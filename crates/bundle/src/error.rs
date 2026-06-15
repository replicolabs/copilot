use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("tip {tip} lamports is below the {min} lamport minimum")]
    TipTooLow { tip: u64, min: u64 },

    #[error("failed to compile transaction message: {0}")]
    Compile(#[from] solana_message::CompileError),

    #[error("failed to sign transaction: {0}")]
    Sign(#[from] solana_signer::SignerError),

    #[error("failed to serialize transaction: {0}")]
    Serialize(#[from] bincode::Error),

    #[error("bundle must contain 1..=5 transactions, got {0}")]
    BundleSize(usize),

    #[error("no Jito tip accounts available")]
    NoTipAccounts,

    #[error("Block Engine request failed: {0}")]
    Rpc(#[from] anyhow::Error),

    #[error("unexpected Block Engine response: {0}")]
    BadResponse(String),
}
