use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::info;

use agent::RetryAgent;
use bundle::{
    BundleStatus, BundleSubmitter, ComputeBudget, MIN_TIP_LAMPORTS, TipConfig, build_transaction,
};
use fault::{
    FailureSignals, MAX_PROCESSING_AGE, OnchainError, SubmissionOutcome, classify,
    inject_expired_blockhash,
};
use geyser::{BlockhashInfo, ChainState, GeyserConfig};
use leader::LeaderTracker;
use lifecycle::{LifecycleEntry, LifecycleLogger, SignatureTracker, TrackerConfig};
use std::sync::Arc;
use tip_oracle::{TipOracle, TipSuggestion};

use crate::banner::{accent, bad, dim, good, warn as warn_style};
use crate::config::Config;

const TIP_ONLY_CU_LIMIT: u32 = 10_000;
const INJECTED_LANDING_DEADLINE: Duration = Duration::from_secs(25);
const LEADER_LOOKAHEAD_SLOTS: u64 = 256;

pub struct Stack {
    state: Arc<ChainState>,
    oracle: TipOracle,
    submitter: BundleSubmitter,
    leader: Arc<LeaderTracker>,
    config: Config,
    cancel: CancellationToken,
    geyser_handle: JoinHandle<Result<(), geyser::Error>>,
    leader_handle: JoinHandle<Result<(), leader::Error>>,
}

pub struct Attempt<'a> {
    pub payer: &'a Keypair,
    pub blockhash: Hash,
    pub tip_lamports: u64,
    pub prio_price: u64,
    pub landing_deadline: Duration,
    pub label: &'a str,
}

impl Stack {
    pub fn launch(config: Config, cancel: CancellationToken) -> Self {
        let rpc = config.rpc_client();
        let (state, geyser_handle) = geyser::spawn(
            GeyserConfig::new(config.grpc_url.clone(), config.grpc_x_token.clone()),
            cancel.clone(),
        );
        let leader = LeaderTracker::new(rpc.clone(), state.clone());
        let leader_handle = tokio::spawn(leader.clone().run(cancel.clone()));
        let oracle = TipOracle::new(rpc);
        let submitter = BundleSubmitter::new(&config.block_engine);

        Self {
            state,
            oracle,
            submitter,
            leader,
            config,
            cancel,
            geyser_handle,
            leader_handle,
        }
    }

    pub fn state(&self) -> &Arc<ChainState> {
        &self.state
    }

    pub fn current_leader(&self) -> Option<Pubkey> {
        self.leader.current_leader()
    }

    pub async fn tip_suggestion(&self) -> Result<TipSuggestion> {
        self.oracle
            .snapshot()
            .await
            .context("sampling the tip oracle")
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.geyser_handle.await;
        let _ = self.leader_handle.await;
    }

    pub async fn await_blockhash(&self) -> Result<Arc<BlockhashInfo>> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut tip = self.state.subscribe_slot_tip();
        loop {
            if let Some(blockhash) = self.state.latest_blockhash() {
                return Ok(blockhash);
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "timed out waiting for the first blockhash from the geyser feed; \
                     check COPILOT_GRPC_URL / token"
                ));
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), tip.changed()).await;
        }
    }

    pub async fn log_leader_window(&self) {
        let current_slot = self.state.processed_slot();
        let jito = fetch_jito_leaders(&self.config.block_engine).await;
        let next = self.leader.schedule().and_then(|schedule| {
            schedule.next_leader_slot(current_slot, &jito, LEADER_LOOKAHEAD_SLOTS)
        });

        match next {
            Some((slot, validator)) => info!(
                current_slot,
                next_jito_slot = slot,
                in_slots = slot.saturating_sub(current_slot),
                leader = %validator,
                "next Jito leader window"
            ),
            None if jito.is_empty() => {
                info!(
                    current_slot,
                    "Jito leader set unavailable; Block Engine will route"
                )
            }
            None => info!(
                current_slot,
                "no Jito leader within lookahead; Block Engine will route"
            ),
        }
    }

    pub async fn submit_attempt(
        &self,
        attempt: Attempt<'_>,
        cancel: &CancellationToken,
    ) -> Result<LifecycleEntry> {
        let Attempt {
            payer,
            blockhash,
            tip_lamports,
            prio_price,
            landing_deadline,
            label,
        } = attempt;

        let tip_account = self
            .submitter
            .tip_accounts()
            .await
            .context("fetching Jito tip accounts")?
            .pick();

        let compute = ComputeBudget {
            unit_limit: TIP_ONLY_CU_LIMIT,
            unit_price_micro_lamports: prio_price,
        };
        let transaction = build_transaction(
            payer,
            Vec::new(),
            compute,
            &TipConfig {
                tip_account,
                tip_lamports,
            },
            blockhash,
        )
        .context("building bundle transaction")?;

        let signature = transaction
            .signatures
            .first()
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("signed transaction carries no signature"))?;
        let submitted_slot = self.state.processed_slot();

        let bundle_id = self
            .submitter
            .submit(&[transaction])
            .await
            .context("submitting bundle to the Block Engine")?;
        info!(%bundle_id, %signature, tip_lamports, label, "bundle submitted");

        let entry =
            LifecycleEntry::submitted(signature, tip_lamports, submitted_slot, Some(bundle_id));

        let mut tracker_config = TrackerConfig::new(
            self.config.grpc_url.clone(),
            self.config.grpc_x_token.clone(),
        );
        tracker_config.landing_deadline = landing_deadline;
        let tracker = SignatureTracker::new(tracker_config, self.state.clone());

        tracker
            .track(entry, cancel.clone())
            .await
            .context("tracking submission lifecycle")
    }

    pub async fn inject_demo(
        &self,
        payer: &Keypair,
        agent: &RetryAgent,
        logger: &LifecycleLogger,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let suggestion = self.tip_suggestion().await?;
        let prio_price = suggestion.prio_fees.percentiles.p50;
        let tip_lamports = suggestion.baseline_tip_lamports().max(MIN_TIP_LAMPORTS);

        let reference = self.await_blockhash().await?;
        let expired = inject_expired_blockhash(reference.blockhash);
        println!(
            "{}",
            warn_style(&format!(
                "→ injecting an expired blockhash (apparent age {} slots, past the {}-slot window)",
                expired.apparent_age_slots, MAX_PROCESSING_AGE
            ))
        );

        let mut failed = self
            .submit_attempt(
                Attempt {
                    payer,
                    blockhash: expired.blockhash,
                    tip_lamports,
                    prio_price,
                    landing_deadline: INJECTED_LANDING_DEADLINE,
                    label: "injected-fault",
                },
                cancel,
            )
            .await?;

        let outcome = match &failed.bundle_id {
            Some(id) => self.outcome_of(id).await,
            None => SubmissionOutcome::Unknown,
        };
        let signals = FailureSignals {
            landed: failed.landed_slot.is_some(),
            blockhash_age_slots: expired.apparent_age_slots,
            max_blockhash_age_slots: MAX_PROCESSING_AGE,
            jito_leader_produced: true,
            outcome,
            tip_lamports,
            recent_landed_tip_p50: suggestion.jito_tip_floor.p50,
            onchain_error: None::<OnchainError>,
        };
        let event = classify(signals);
        failed.record_failure(format!("{:?}", event.kind));
        let failed_path = logger
            .write(&failed)
            .context("writing failed lifecycle log")?;
        println!(
            "  classified {} (confidence {:.0}%) — {}",
            accent(&format!("{:?}", event.kind)),
            event.confidence * 100.0,
            dim(&event.rationale)
        );
        info!(log = %failed_path.display(), "failed attempt logged");

        let context = json!({
            "failure": event,
            "chain": {
                "processed_slot": self.state.processed_slot(),
                "confirmed_slot": self.state.confirmed_slot(),
                "finalized_slot": self.state.finalized_slot(),
            },
            "tips": suggestion,
            "failed_attempt": {
                "signature": failed.signature,
                "tip_lamports": failed.tip_lamports,
                "landed": failed.landed_slot.is_some(),
            },
        });
        let decision = agent.decide(&context).await.context("agent decision")?;
        println!(
            "  agent decision: {} (confidence {:.0}%) — {}",
            accent(&format!("{:?}", decision.action)),
            decision.confidence * 100.0,
            dim(&decision.reasoning)
        );

        if !decision.should_retry() {
            println!("{}", warn_style("  agent chose not to retry."));
            return Ok(());
        }

        let fresh = self.await_blockhash().await?;
        let retry_tip = decision
            .new_tip_lamports
            .unwrap_or(tip_lamports)
            .max(MIN_TIP_LAMPORTS);
        println!(
            "{}",
            dim(&format!(
                "  retrying with a fresh blockhash and a {retry_tip} lamport tip…"
            ))
        );

        let landed = self
            .submit_attempt(
                Attempt {
                    payer,
                    blockhash: fresh.blockhash,
                    tip_lamports: retry_tip,
                    prio_price,
                    landing_deadline: lifecycle::DEFAULT_LANDING_DEADLINE,
                    label: "agent-retry",
                },
                cancel,
            )
            .await?;
        let landed_path = logger
            .write(&landed)
            .context("writing retry lifecycle log")?;

        match landed.landed_slot {
            Some(slot) => println!(
                "{}",
                good(&format!(
                    "  ✓ retry landed in slot {slot} — logged to {}",
                    landed_path.display()
                ))
            ),
            None => println!(
                "{}",
                bad("  ✗ retry did not land within the deadline (see the log for the timeline)")
            ),
        }
        Ok(())
    }

    async fn outcome_of(&self, bundle_id: &str) -> SubmissionOutcome {
        match self.submitter.inflight_status(bundle_id).await {
            Ok(BundleStatus::Landed { .. }) => SubmissionOutcome::Landed,
            Ok(BundleStatus::Failed) => SubmissionOutcome::Failed,
            Ok(BundleStatus::Invalid) => SubmissionOutcome::Rejected,
            Ok(BundleStatus::Pending) => SubmissionOutcome::Submitted,
            Err(_) => SubmissionOutcome::Unknown,
        }
    }
}

async fn fetch_jito_leaders(block_engine: &str) -> HashSet<Pubkey> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getConnectedLeaders",
        "params": []
    });

    let attempt = async {
        let response = reqwest::Client::new()
            .post(block_engine)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        let map = value
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("getConnectedLeaders: missing result map"))?;
        Ok::<HashSet<Pubkey>, anyhow::Error>(
            map.keys().filter_map(|key| key.parse().ok()).collect(),
        )
    };

    attempt.await.unwrap_or_default()
}
