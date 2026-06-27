use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::ClientTlsConfig;
use tracing::{debug, info, warn};
use yellowstone_grpc_client::{Backoff, GeyserGrpcClient, ReconnectConfig};
use yellowstone_grpc_proto::prelude::{
    CommitmentLevel as ProtoCommitment, SlotStatus, SubscribeRequest,
    SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, subscribe_update::UpdateOneof,
};

use crate::{ChainState, Error};

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

impl Commitment {
    fn to_proto(self) -> i32 {
        let level = match self {
            Commitment::Processed => ProtoCommitment::Processed,
            Commitment::Confirmed => ProtoCommitment::Confirmed,
            Commitment::Finalized => ProtoCommitment::Finalized,
        };
        level as i32
    }
}

#[derive(Debug, Clone)]
pub struct GeyserConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub commitment: Commitment,
}

impl GeyserConfig {
    pub fn new(endpoint: impl Into<String>, x_token: Option<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            x_token,
            commitment: Commitment::Processed,
        }
    }
}

pub fn spawn(
    config: GeyserConfig,
    cancel: CancellationToken,
) -> (Arc<ChainState>, JoinHandle<Result<(), Error>>) {
    let state = ChainState::new();
    let task = tokio::spawn(run(config, Arc::clone(&state), cancel));
    (state, task)
}

fn reconnect_config() -> ReconnectConfig {
    ReconnectConfig::default().with_backoff(Backoff::new(Duration::from_millis(500), 2.0, 10))
}

fn build_request(commitment: Commitment, tracked_sig: Option<&str>) -> SubscribeRequest {
    let mut slots = HashMap::with_capacity(1);
    slots.insert(
        "slots".to_owned(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: Some(false),
            interslot_updates: Some(false),
        },
    );

    let mut blocks_meta = HashMap::with_capacity(1);
    blocks_meta.insert("blockmeta".to_owned(), SubscribeRequestFilterBlocksMeta {});

    let mut transactions = HashMap::new();
    if let Some(sig) = tracked_sig {
        transactions.insert(
            "sig".to_owned(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: None,
                signature: Some(sig.to_owned()),
                account_include: Vec::new(),
                account_exclude: Vec::new(),
                account_required: Vec::new(),
            },
        );
    }

    SubscribeRequest {
        slots,
        blocks_meta,
        transactions,
        commitment: Some(commitment.to_proto()),
        ..Default::default()
    }
}

async fn connect(config: &GeyserConfig) -> Result<GeyserGrpcClient, Error> {
    let mut builder = GeyserGrpcClient::build_from_shared(config.endpoint.clone())?
        .x_token(config.x_token.clone())?
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .set_reconnect_config(reconnect_config());

    if config.endpoint.starts_with("https") {
        builder = builder.tls_config(ClientTlsConfig::new().with_native_roots())?;
    }

    Ok(builder.connect().await?)
}

async fn run(
    config: GeyserConfig,
    state: Arc<ChainState>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    let mut backoff_ms = 500u64;
    const MAX_BACKOFF_MS: u64 = 30_000;

    loop {
        let mut got_message = false;
        match run_once(&config, &state, &cancel, &mut got_message).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!(%e, backoff_ms, "Geyser stream lost; reconnecting");
                if got_message {
                    backoff_ms = 500;
                }
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(()),
                    _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {}
                }
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
            }
        }
    }
}

async fn run_once(
    config: &GeyserConfig,
    state: &Arc<ChainState>,
    cancel: &CancellationToken,
    got_message: &mut bool,
) -> Result<(), Error> {
    info!(endpoint = %config.endpoint, "connecting to Geyser");
    let mut client = connect(config).await?;

    let mut tracked_sig = state.subscribe_tracked_sig();
    let initial_sig = tracked_sig.borrow_and_update().clone();
    let mut current_sig = initial_sig.clone();

    let (mut sink, mut stream) = client
        .subscribe_with_request(Some(build_request(
            config.commitment,
            initial_sig.as_deref(),
        )))
        .await
        .map_err(|e| {
            tracing::error!("Geyser subscription rejected by server: {e}");
            e
        })?;
    info!("Geyser subscription established");

    let _client = client;

    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Geyser subscriber shutting down");
                return Ok(());
            }

            _ = keepalive.tick() => {
                let ping = SubscribeRequest {
                    ping: Some(SubscribeRequestPing { id: 1 }),
                    ..Default::default()
                };
                if let Err(err) = sink.send(ping).await {
                    debug!(%err, "keepalive ping failed; awaiting stream error");
                }
            }

            _ = tracked_sig.changed() => {
                let new_sig = tracked_sig.borrow_and_update().clone();
                if new_sig != current_sig {
                    current_sig = new_sig.clone();
                    let req = build_request(config.commitment, current_sig.as_deref());
                    if let Err(err) = sink.send(req).await {
                        debug!(%err, "failed to update subscription filter");
                    }
                }
            }

            message = stream.next() => match message {
                Some(Ok(update)) => {
                    *got_message = true;
                    apply(state, update);
                }
                Some(Err(status)) => {
                    warn!(%status, "Geyser stream error");
                    return Err(Error::Stream(status));
                }
                None => {
                    warn!("Geyser stream closed by server");
                    return Err(Error::Closed);
                }
            },
        }
    }
}

fn apply(state: &ChainState, update: yellowstone_grpc_proto::prelude::SubscribeUpdate) {
    match update.update_oneof {
        Some(UpdateOneof::Slot(slot)) => {
            if let Ok(status) = SlotStatus::try_from(slot.status) {
                state.record_slot(slot.slot, status);
            }
        }
        Some(UpdateOneof::BlockMeta(meta)) => match meta.blockhash.parse() {
            Ok(blockhash) => state.record_blockhash(blockhash, meta.slot),
            Err(err) => debug!(%err, blockhash = %meta.blockhash, "skipping unparseable blockhash"),
        },
        Some(UpdateOneof::Transaction(tx)) => {
            state.notify_transaction_landed(tx.slot);
        }
        _ => {}
    }
}
