# Bi-directional streaming with gRPC

So I wanted to stream messages over gRPC using tonic, mainly to take advantage of a protocol when streaming. It seems fairly simple if you look at the protobuf definition of what a bidirectional or a unidirectional stream would look like, the implementation threw me off a little because I didn’t know what the notion of a stream would be in this case.

Let’s create a simple protocol with an RPC that consumes and returns a stream.

```protobuf
syntax = "proto3";

package messages;

service MessageService {  
  rpc StreamMessages(stream MessageRequest) returns (stream MessageResponse);
}

message MessageRequest {
  string name = 1;
}

message MessageResponse {
  string message = 1;
}
```

Then, let’s generate our types from the protocol definition by adding this to our `build.rs`.

```rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/messages.proto")?;

    Ok(())
}
```

And then, implement a basic blueprint of our server.

```rs
pub mod messages {
    tonic::include_proto!("messages");
}

#[derive(Debug, Error)]
pub enum ServerError {
    ...
}

#[derive(Debug, Default)]
pub struct MyMessageService;

#[tonic::async_trait]
impl MessageService for MyMessageService {
    type StreamMessagesStream = ??        

    #[tracing::instrument]
    async fn stream_messages(
        &self,
        request: Request<tonic::Streaming<MessageRequest>>,
    ) -> Result<tonic::Response<Self::StreamMessagesStream>, tonic::Status> {
       	todo!()
}

#[tracing::instrument]
pub async fn run() -> Result<(), ServerError> {
   ...
}
```

The `stream_messages` procedure returns a `Result<tonic::Response<Self::StreamMessagesStream>, tonic::Status>` where our associated type must be a `Stream`. To suit the case of this application I want to send messages continuously to the connected client like a stream would be. But `Stream` is asynchronous i.e. the values are produced asynchronously. To do so, I came across Tokio’s implementation of `mpsc` whose Receiver end has a wrapper that implements `Stream`, which means I can just return the receiver of the `mpsc` channel!


The generated `prost` associated type for StreamMessagesStream is an alias for a stream of message responses:

```rs
type StreamMessagesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<super::MessageResponse, tonic::Status>,
            >
            + Send
            + 'static;
```

And since we return the receiver end of the channel through a ReceiverStream, a receiver type that implements `Send` we have to use `Box` and dynamic dispatch with `Pin` (because of futures).

```rs
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
```

Similarly to mimic a bidirectional stream, our client also needs to send a stream of messages which needs to be consumed by the server while the client consumes the server’s stream.

```rs
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
```

There we go, now we can get two people to talk over gRPC! I needed to do this as I am implementing MapReduce in Rust and I don’t want to have both the client and server listening over different addresses and then doing RPC calls and having to go through establishing a connection each time. I instead want my clients to stay connected to the master while taking advantage of the protocol. Something like this:


```protobuf
message JobMessage {
    oneof message {
        Job job = 1;
        JobReply job_reply = 2;
        Shutdown shutdown = 3;
        ShutdownReply shutdown_reply = 4;
    }
}
```


Well, maybe I am not thinking this right or might have made a mistake somewhere. So please correct me if so! And thus I conclude with a takeaway that Tokio docs are amazing and every minute spent on them is worth it!
