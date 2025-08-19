use anyhow::Context;
use web_transport_generic::{RecvStream, SendStream, Session as _};
use web_transport_polyfill::Session;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "ws://127.0.0.1:3000";
    println!("Connecting to {url}");

    let session = Session::connect(url).await?;
    println!("WebSocket connection established");

    println!("\n=== Testing unidirectional stream ===");
    let mut uni_stream = session.open_uni().await?;
    uni_stream
        .write(b"Hello from unidirectional stream!")
        .await?;
    uni_stream.finish().await?;
    println!("Sent message on unidirectional stream");

    // Receive back the same message
    let mut recv = session.accept_uni().await?;
    let data = recv.read().await?.context("Failed to read message")?;
    println!("Received: {}", String::from_utf8_lossy(&data));

    println!("\n=== Testing bidirectional stream ===");
    let (mut send, mut recv) = session.open_bi().await?;

    let message = b"Hello from bidirectional stream!";
    send.write(message).await?;
    println!("Sent: Hello from bidirectional stream!");

    if let Ok(Some(response)) = recv.read().await {
        let text = String::from_utf8_lossy(&response);
        println!("Received: {text}");
    }

    send.finish().await?;

    println!("\nClient shutting down...");
    Ok(())
}
