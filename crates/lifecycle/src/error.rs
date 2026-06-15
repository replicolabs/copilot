use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to build/connect tracking client: {0}")]
    Connect(#[from] yellowstone_grpc_client::GeyserGrpcBuilderError),

    #[error("failed to open signature subscription: {0}")]
    Subscribe(#[from] yellowstone_grpc_client::GeyserGrpcClientError),

    #[error("tracking stream error: {0}")]
    Stream(#[from] tonic::Status),

    #[error("tracking stream closed by server")]
    Closed,

    #[error("lifecycle log I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("lifecycle log serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}
