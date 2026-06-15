use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("tip-floor request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("prioritization-fee RPC failed: {0}")]
    Rpc(#[from] solana_rpc_client_api::client_error::Error),

    #[error("tip-floor response was malformed: {0}")]
    Malformed(&'static str),
}
