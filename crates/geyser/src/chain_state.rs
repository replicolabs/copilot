use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwapOption;
use solana_hash::Hash;
use tokio::sync::watch;
use yellowstone_grpc_proto::prelude::SlotStatus;

#[derive(Debug, Clone, Copy)]
pub struct BlockhashInfo {
    pub blockhash: Hash,
    pub slot: u64,
}

#[derive(Debug)]
pub struct ChainState {
    processed_slot: AtomicU64,
    confirmed_slot: AtomicU64,
    finalized_slot: AtomicU64,
    blockhash: ArcSwapOption<BlockhashInfo>,
    tip_tx: watch::Sender<u64>,
}

impl ChainState {
    pub fn new() -> Arc<Self> {
        let (tip_tx, _) = watch::channel(0);
        Arc::new(Self {
            processed_slot: AtomicU64::new(0),
            confirmed_slot: AtomicU64::new(0),
            finalized_slot: AtomicU64::new(0),
            blockhash: ArcSwapOption::empty(),
            tip_tx,
        })
    }

    #[inline]
    pub fn processed_slot(&self) -> u64 {
        self.processed_slot.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn confirmed_slot(&self) -> u64 {
        self.confirmed_slot.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn finalized_slot(&self) -> u64 {
        self.finalized_slot.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn latest_blockhash(&self) -> Option<Arc<BlockhashInfo>> {
        self.blockhash.load_full()
    }

    pub fn subscribe_slot_tip(&self) -> watch::Receiver<u64> {
        self.tip_tx.subscribe()
    }

    pub(crate) fn record_slot(&self, slot: u64, status: SlotStatus) {
        match status {
            SlotStatus::SlotProcessed => {
                let previous = self.processed_slot.fetch_max(slot, Ordering::Relaxed);
                if slot > previous {
                    let _ = self.tip_tx.send(slot);
                }
            }
            SlotStatus::SlotConfirmed => {
                self.confirmed_slot.fetch_max(slot, Ordering::Relaxed);
            }
            SlotStatus::SlotFinalized => {
                self.finalized_slot.fetch_max(slot, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub(crate) fn record_blockhash(&self, blockhash: Hash, slot: u64) {
        let is_newer = self
            .blockhash
            .load()
            .as_ref()
            .is_none_or(|current| slot > current.slot);
        if is_newer {
            self.blockhash
                .store(Some(Arc::new(BlockhashInfo { blockhash, slot })));
        }
    }
}
