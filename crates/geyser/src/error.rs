use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to build/connect Geyser client: {0}")]
    Connect(#[from] yellowstone_grpc_client::GeyserGrpcBuilderError),

    #[error("failed to open subscription: {0}")]
    Subscribe(#[from] yellowstone_grpc_client::GeyserGrpcClientError),

    #[error("subscription stream error: {0}")]
    Stream(#[from] tonic::Status),

    #[error("subscription stream closed by server")]
    Closed,
}
