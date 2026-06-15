use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use geyser::ChainState;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tonic::transport::ClientTlsConfig;
use tracing::{debug, info};
use yellowstone_grpc_client::{Backoff, GeyserGrpcClient, ReconnectConfig};
use yellowstone_grpc_proto::prelude::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterTransactions, SubscribeRequestPing,
    subscribe_update::UpdateOneof,
};

use crate::{Error, entry::LifecycleEntry};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
pub const DEFAULT_LANDING_DEADLINE: Duration = Duration::from_secs(90);
pub const DEFAULT_FINALIZE_DEADLINE: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub landing_deadline: Duration,
    pub finalize_deadline: Duration,
}

impl TrackerConfig {
    pub fn new(endpoint: impl Into<String>, x_token: Option<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            x_token,
            landing_deadline: DEFAULT_LANDING_DEADLINE,
            finalize_deadline: DEFAULT_FINALIZE_DEADLINE,
        }
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
        let landed_slot = match self.await_landing(&mut entry, &cancel).await? {
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
    ) -> Result<Option<u64>, Error> {
        let mut client = self.connect().await?;
        let (mut sink, mut stream) = client
            .subscribe_with_request(Some(self.landing_request(&entry.signature)))
            .await?;
        info!(signature = %entry.signature, "tracking submission for landing");

        let deadline = tokio::time::sleep(self.config.landing_deadline);
        tokio::pin!(deadline);
        let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
        keepalive.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(None),
                _ = &mut deadline => {
                    debug!(signature = %entry.signature, "landing deadline elapsed; never observed");
                    return Ok(None);
                }
                _ = keepalive.tick() => {
                    let ping = SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    };
                    if let Err(err) = sink.send(ping).await {
                        debug!(%err, "tracker keepalive ping failed");
                    }
                }
                message = stream.next() => match message {
                    Some(Ok(update)) => {
                        if let Some(UpdateOneof::Transaction(tx)) = update.update_oneof {
                            entry.record_processed(tx.slot);
                            info!(signature = %entry.signature, slot = tx.slot, "transaction landed");
                            return Ok(Some(tx.slot));
                        }
                    }
                    Some(Err(status)) => return Err(Error::Stream(status)),
                    None => return Err(Error::Closed),
                },
            }
        }
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

    fn landing_request(&self, signature: &str) -> SubscribeRequest {
        let mut transactions = HashMap::with_capacity(1);
        transactions.insert(
            "sig".to_owned(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: None,
                signature: Some(signature.to_owned()),
                account_include: Vec::new(),
                account_exclude: Vec::new(),
                account_required: Vec::new(),
            },
        );
        SubscribeRequest {
            transactions,
            commitment: Some(CommitmentLevel::Processed as i32),
            ..Default::default()
        }
    }

    async fn connect(&self) -> Result<GeyserGrpcClient, Error> {
        let mut builder = GeyserGrpcClient::build_from_shared(self.config.endpoint.clone())?
            .x_token(self.config.x_token.clone())?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .set_reconnect_config(ReconnectConfig::default().with_backoff(Backoff::new(
                Duration::from_millis(500),
                2.0,
                10,
            )));

        if self.config.endpoint.starts_with("https") {
            builder = builder.tls_config(ClientTlsConfig::new().with_native_roots())?;
        }
        Ok(builder.connect().await?)
    }
}
