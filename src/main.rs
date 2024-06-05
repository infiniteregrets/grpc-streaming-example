use tracing::info;

mod client;
mod server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting server and client");
    let server_handle = tokio::spawn(server::run());
    let client_handle = tokio::spawn(client::run());

    let _ = tokio::try_join!(server_handle, client_handle);
}
