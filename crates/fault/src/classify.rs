use serde::Serialize;

use crate::signals::{FailureSignals, OnchainError, SubmissionOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    ExpiredBlockhash,
    FeeTooLow,
    ComputeExceeded,
    BundleFailure,
    LeaderSkipped,
    Dropped,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureEvent {
    pub kind: FailureKind,
    pub confidence: f64,
    pub rationale: String,
    pub alternatives: Vec<FailureKind>,
    pub signals: FailureSignals,
}

pub fn classify(signals: FailureSignals) -> FailureEvent {
    let (kind, confidence, rationale, alternatives) = decide(&signals);
    FailureEvent {
        kind,
        confidence,
        rationale,
        alternatives,
        signals,
    }
}

fn decide(s: &FailureSignals) -> (FailureKind, f64, String, Vec<FailureKind>) {
    if let Some(error) = &s.onchain_error {
        return match error {
            OnchainError::ComputeExceeded => (
                FailureKind::ComputeExceeded,
                1.0,
                "transaction landed but exhausted its compute-unit budget".into(),
                vec![],
            ),
            OnchainError::InsufficientFunds => (
                FailureKind::BundleFailure,
                0.95,
                "transaction landed but the fee payer had insufficient funds".into(),
                vec![],
            ),
            OnchainError::InstructionFailed => (
                FailureKind::BundleFailure,
                0.85,
                "transaction landed but an instruction returned an error".into(),
                vec![],
            ),
            OnchainError::Other(detail) => (
                FailureKind::BundleFailure,
                0.8,
                format!("transaction landed but failed on-chain: {detail}"),
                vec![FailureKind::Unknown],
            ),
        };
    }

    if s.landed {
        return (
            FailureKind::Unknown,
            0.0,
            "transaction landed without error; not a failure".into(),
            vec![],
        );
    }

    if s.blockhash_expired() {
        let mut alts = vec![];
        if !s.jito_leader_produced {
            alts.push(FailureKind::LeaderSkipped);
        }
        return (
            FailureKind::ExpiredBlockhash,
            0.9,
            format!(
                "blockhash aged {} slots, past the {}-slot window, before inclusion",
                s.blockhash_age_slots, s.max_blockhash_age_slots
            ),
            alts,
        );
    }

    if !s.jito_leader_produced {
        return (
            FailureKind::LeaderSkipped,
            0.8,
            "the target Jito leader did not produce a block; the bundle was dropped".into(),
            vec![FailureKind::Dropped],
        );
    }

    if matches!(
        s.outcome,
        SubmissionOutcome::Failed | SubmissionOutcome::Rejected
    ) {
        return (
            FailureKind::BundleFailure,
            0.7,
            "the Block Engine failed or rejected the bundle (auction or simulation)".into(),
            vec![FailureKind::FeeTooLow],
        );
    }

    if s.tip_below_median() {
        return (
            FailureKind::FeeTooLow,
            0.6,
            format!(
                "tip of {} lamports was below the recently-landed median of {}",
                s.tip_lamports, s.recent_landed_tip_p50
            ),
            vec![FailureKind::Dropped],
        );
    }

    (
        FailureKind::Dropped,
        0.4,
        "never landed, with no decisive cause; tip looked adequate and the leader produced".into(),
        vec![FailureKind::FeeTooLow, FailureKind::LeaderSkipped],
    )
}
