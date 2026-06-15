use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("leader-schedule RPC failed: {0}")]
    Rpc(#[from] solana_rpc_client_api::client_error::Error),
    #[error("no leader schedule available for slot {0}")]
    ScheduleUnavailable(u64),
    #[error("invalid validator identity in schedule: {0}")]
    InvalidIdentity(String),
    #[error("leader schedule has too many unique validators to index")]
    TooManyLeaders,
}
