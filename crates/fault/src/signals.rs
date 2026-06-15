use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionOutcome {
    Submitted,
    Failed,
    Rejected,
    Landed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnchainError {
    ComputeExceeded,
    InsufficientFunds,
    InstructionFailed,
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureSignals {
    pub landed: bool,
    pub blockhash_age_slots: u64,
    pub max_blockhash_age_slots: u64,
    pub jito_leader_produced: bool,
    pub outcome: SubmissionOutcome,
    pub tip_lamports: u64,
    pub recent_landed_tip_p50: u64,
    pub onchain_error: Option<OnchainError>,
}

impl FailureSignals {
    pub fn blockhash_expired(&self) -> bool {
        self.blockhash_age_slots > self.max_blockhash_age_slots
    }

    pub fn tip_below_median(&self) -> bool {
        self.tip_lamports < self.recent_landed_tip_p50
    }
}
