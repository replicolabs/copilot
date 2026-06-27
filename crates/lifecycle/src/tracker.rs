use std::sync::Arc;
use std::time::Duration;

use geyser::ChainState;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{Error, entry::LifecycleEntry};

pub const DEFAULT_LANDING_DEADLINE: Duration = Duration::from_secs(90);
pub const DEFAULT_FINALIZE_DEADLINE: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub landing_deadline: Duration,
    pub finalize_deadline: Duration,
    pub rpc_url: Option<String>,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackerConfig {
    pub fn new() -> Self {
        Self {
            landing_deadline: DEFAULT_LANDING_DEADLINE,
            finalize_deadline: DEFAULT_FINALIZE_DEADLINE,
            rpc_url: None,
        }
    }

    pub fn with_rpc_fallback(mut self, url: String) -> Self {
        self.rpc_url = Some(url);
        self
    }
}

pub struct SignatureTracker {
    config: TrackerConfig,
    state: Arc<ChainState>,
}

impl SignatureTracker {
    pub fn new(config: TrackerConfig, state: Arc<ChainState>) -> Self {
        Self { config, state }
    }

    pub async fn track(
        &self,
        mut entry: LifecycleEntry,
        cancel: CancellationToken,
    ) -> Result<LifecycleEntry, Error> {
        let landed_slot = match self.await_landing(&mut entry, &cancel).await {
            Some(slot) => slot,
            None => return Ok(entry),
        };
        self.await_commitments(&mut entry, landed_slot, &cancel)
            .await;
        Ok(entry)
    }

    async fn await_landing(
        &self,
        entry: &mut LifecycleEntry,
        cancel: &CancellationToken,
    ) -> Option<u64> {
        self.state
            .set_tracked_signature(Some(entry.signature.clone()));
        let mut landing = self.state.subscribe_landing();

        info!(signature = %entry.signature, "tracking submission for landing");

        let deadline = tokio::time::sleep(self.config.landing_deadline);
        tokio::pin!(deadline);

        let slot = tokio::select! {
            _ = cancel.cancelled() => None,
            _ = &mut deadline => {
                debug!(signature = %entry.signature, "landing deadline elapsed; never observed");
                self.rpc_fallback(&entry.signature).await
            }
            result = landing.recv() => match result {
                Ok(slot) => {
                    entry.record_processed(slot);
                    info!(signature = %entry.signature, slot, "transaction landed");
                    Some(slot)
                }
                Err(_) => None,
            },
        };

        self.state.set_tracked_signature(None);
        if let Some(slot) = slot
            && entry.processed_at.is_none()
        {
            entry.record_processed(slot);
            info!(signature = %entry.signature, slot, "transaction landed (rpc fallback)");
        }
        slot
    }

    async fn rpc_fallback(&self, signature: &str) -> Option<u64> {
        let url = self.config.rpc_url.as_deref()?;

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[signature], {"searchTransactionHistory": true}]
        });

        let resp = match reqwest::Client::new().post(url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!(%e, "rpc fallback: request failed");
                return None;
            }
        };
        let result: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!(%e, "rpc fallback: failed to parse response");
                return None;
            }
        };

        let slot = result
            .pointer("/result/value/0/slot")
            .and_then(|v| v.as_u64());

        if slot.is_some() {
            info!(signature, slot, "rpc fallback: transaction found on-chain");
        } else {
            debug!(signature, "rpc fallback: transaction not found");
        }

        slot
    }

    async fn await_commitments(
        &self,
        entry: &mut LifecycleEntry,
        landed_slot: u64,
        cancel: &CancellationToken,
    ) {
        let mut tip = self.state.subscribe_slot_tip();
        let deadline = tokio::time::sleep(self.config.finalize_deadline);
        tokio::pin!(deadline);

        self.record_reached_commitments(entry, landed_slot);

        while entry.finalized_at.is_none() {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = &mut deadline => {
                    debug!(signature = %entry.signature, "finalize deadline elapsed");
                    break;
                }
                changed = tip.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    self.record_reached_commitments(entry, landed_slot);
                }
            }
        }
    }

    fn record_reached_commitments(&self, entry: &mut LifecycleEntry, landed_slot: u64) {
        if entry.confirmed_at.is_none() && self.state.confirmed_slot() >= landed_slot {
            entry.record_confirmed();
            debug!(signature = %entry.signature, slot = landed_slot, "reached confirmed");
        }
        if entry.finalized_at.is_none() && self.state.finalized_slot() >= landed_slot {
            entry.record_finalized();
            debug!(signature = %entry.signature, slot = landed_slot, "reached finalized");
        }
    }
}
