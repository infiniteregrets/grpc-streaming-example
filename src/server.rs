use futures::Stream;
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::error;
use tracing::info;

use crate::server::messages::MessageRequest;
use messages::message_service_server::{MessageService, MessageServiceServer};

pub mod messages {
    tonic::include_proto!("messages");
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("Address parsing error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),
}

#[derive(Debug, Default)]
pub struct MyMessageService;

#[tonic::async_trait]
impl MessageService for MyMessageService {
    type StreamMessagesStream =
        Pin<Box<dyn Stream<Item = Result<messages::MessageResponse, tonic::Status>> + Send + Sync>>;

    #[tracing::instrument]
    async fn stream_messages(
        &self,
        request: Request<tonic::Streaming<MessageRequest>>,
    ) -> Result<tonic::Response<Self::StreamMessagesStream>, tonic::Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let mut stream = request.into_inner();
            while let Some(request) = stream.message().await? {
                info!("Received request: {:?}", request);
            }
            Ok::<(), Status>(())
        });

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let message = messages::MessageResponse {
                    message: "Hello, world!".into(),
                };
                if let Err(e) = tx.send(Ok(message)).await {
                    error!("Failed to send message: {:?}", e);
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
}

#[tracing::instrument]
pub async fn run() -> Result<(), ServerError> {
    let addr = "[::1]:50051".parse()?;
    let svc = MessageServiceServer::new(MyMessageService {});

    Server::builder().add_service(svc).serve(addr).await?;

    Ok(())
}
