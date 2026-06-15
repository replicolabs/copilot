use serde::{Deserialize, Serialize};

pub type Millis = u64;

pub fn now_millis() -> Millis {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Submitted,
    Processed,
    Confirmed,
    Finalized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEntry {
    pub bundle_id: Option<String>,
    pub signature: String,
    pub tip_lamports: u64,
    pub submitted_at: Millis,
    pub submitted_slot: u64,
    pub landed_slot: Option<u64>,
    pub processed_at: Option<Millis>,
    pub confirmed_at: Option<Millis>,
    pub finalized_at: Option<Millis>,
    pub failure: Option<String>,
}

impl LifecycleEntry {
    pub fn submitted(
        signature: String,
        tip_lamports: u64,
        submitted_slot: u64,
        bundle_id: Option<String>,
    ) -> Self {
        Self {
            bundle_id,
            signature,
            tip_lamports,
            submitted_at: now_millis(),
            submitted_slot,
            landed_slot: None,
            processed_at: None,
            confirmed_at: None,
            finalized_at: None,
            failure: None,
        }
    }

    pub fn record_processed(&mut self, landed_slot: u64) {
        self.landed_slot = Some(landed_slot);
        self.processed_at.get_or_insert_with(now_millis);
    }

    pub fn record_confirmed(&mut self) {
        self.confirmed_at.get_or_insert_with(now_millis);
    }

    pub fn record_finalized(&mut self) {
        self.finalized_at.get_or_insert_with(now_millis);
    }

    pub fn record_failure(&mut self, classification: impl Into<String>) {
        self.failure = Some(classification.into());
    }

    pub fn stage(&self) -> Stage {
        if self.finalized_at.is_some() {
            Stage::Finalized
        } else if self.confirmed_at.is_some() {
            Stage::Confirmed
        } else if self.processed_at.is_some() {
            Stage::Processed
        } else {
            Stage::Submitted
        }
    }

    pub fn submitted_to_processed_ms(&self) -> Option<u64> {
        self.processed_at
            .map(|p| p.saturating_sub(self.submitted_at))
    }

    pub fn processed_to_confirmed_ms(&self) -> Option<u64> {
        match (self.processed_at, self.confirmed_at) {
            (Some(p), Some(c)) => Some(c.saturating_sub(p)),
            _ => None,
        }
    }

    pub fn confirmed_to_finalized_ms(&self) -> Option<u64> {
        match (self.confirmed_at, self.finalized_at) {
            (Some(c), Some(f)) => Some(f.saturating_sub(c)),
            _ => None,
        }
    }
}
