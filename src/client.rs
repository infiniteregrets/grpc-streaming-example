pub mod messages {
    tonic::include_proto!("messages");
}

use messages::message_service_client::MessageServiceClient;
use std::time::Duration;
use thiserror::Error;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("Status error: {0}")]
    Status(#[from] tonic::Status),
    #[error("Send error: {0}")]
    Send(#[from] tokio::sync::mpsc::error::SendError<messages::MessageRequest>),
}

#[tracing::instrument]
pub async fn run() -> Result<(), ClientError> {
    let mut client = MessageServiceClient::connect("http://[::1]:50051").await?;
    let (tx, rx) = tokio::sync::mpsc::channel(4);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let message = messages::MessageRequest {
                name: "Hello, world!".into(),
            };
            if let Err(e) = tx.send(message).await {
                error!("Failed to send message: {:?}", e);
                break;
            }
        }
    });

    let request = tonic::Request::new(ReceiverStream::new(rx));
    let response = client.stream_messages(request).await?;
    let mut stream = response.into_inner();

    while let Some(message) = stream.message().await? {
        info!("Received message: {:?}", message);
    }

    Ok(())
}
