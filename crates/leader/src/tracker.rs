use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use geyser::ChainState;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{Error, LeaderSchedule};

const REFRESH_GUARD_INTERVAL: Duration = Duration::from_secs(30);

pub struct LeaderTracker {
    rpc: Arc<RpcClient>,
    state: Arc<ChainState>,
    schedule: ArcSwapOption<LeaderSchedule>,
}

impl LeaderTracker {
    pub fn new(rpc: Arc<RpcClient>, state: Arc<ChainState>) -> Arc<Self> {
        Arc::new(Self {
            rpc,
            state,
            schedule: ArcSwapOption::empty(),
        })
    }

    pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<(), Error> {
        if let Err(err) = self.refresh().await {
            warn!(%err, "initial leader-schedule load failed; will retry");
        }

        let mut tip = self.state.subscribe_slot_tip();
        let mut guard = tokio::time::interval(REFRESH_GUARD_INTERVAL);
        guard.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("leader tracker shutting down");
                    return Ok(());
                }
                _ = guard.tick() => self.refresh_if_needed().await,
                changed = tip.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    self.refresh_if_needed().await;
                }
            }
        }
    }

    pub fn current_leader(&self) -> Option<Pubkey> {
        self.leader_at(self.state.processed_slot())
    }

    pub fn leader_at(&self, slot: u64) -> Option<Pubkey> {
        self.schedule.load_full()?.leader_at(slot).copied()
    }

    pub fn schedule(&self) -> Option<Arc<LeaderSchedule>> {
        self.schedule.load_full()
    }

    async fn refresh_if_needed(&self) {
        if self.needs_refresh()
            && let Err(err) = self.refresh().await
        {
            warn!(%err, "leader-schedule refresh failed; will retry");
        }
    }

    fn needs_refresh(&self) -> bool {
        let slot = self.state.processed_slot();
        match self.schedule.load_full() {
            None => true,
            Some(schedule) => slot != 0 && !schedule.contains_slot(slot),
        }
    }

    async fn refresh(&self) -> Result<(), Error> {
        let info = self.rpc.get_epoch_info().await?;

        if let Some(existing) = self.schedule.load_full()
            && existing.epoch() == info.epoch
        {
            return Ok(());
        }

        let first_slot = info.absolute_slot.saturating_sub(info.slot_index);
        let raw = self
            .rpc
            .get_leader_schedule(Some(first_slot))
            .await?
            .ok_or(Error::ScheduleUnavailable(first_slot))?;

        let schedule = LeaderSchedule::build(info.epoch, first_slot, info.slots_in_epoch, raw)?;
        info!(
            epoch = info.epoch,
            first_slot, "loaded leader schedule for epoch"
        );
        self.schedule.store(Some(Arc::new(schedule)));
        Ok(())
    }
}
