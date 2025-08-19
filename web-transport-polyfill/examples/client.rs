use tokio_tungstenite::connect_async;
use web_transport_polyfill::Session;
use web_transport_generic::{Session as _, RecvStream, SendStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "ws://127.0.0.1:3000";
    println!("Connecting to {}", url);

    let (ws_stream, _) = connect_async(url).await?;
    println!("WebSocket connection established");

    let mut session = Session::new(ws_stream, false);

    println!("\n=== Testing unidirectional stream ===");
    let mut uni_stream = session.open_uni().await?;
    uni_stream.write(b"Hello from unidirectional stream!").await?;
    uni_stream.finish().await?;
    println!("Sent message on unidirectional stream");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\n=== Testing bidirectional stream ===");
    let (mut send, mut recv) = session.open_bi().await?;
    
    let message = b"Hello from bidirectional stream!";
    send.write(message).await?;
    println!("Sent: Hello from bidirectional stream!");
    
    if let Ok(Some(response)) = recv.read().await {
        let text = String::from_utf8_lossy(&response);
        println!("Received: {}", text);
    }
    
    send.finish().await?;
    
    println!("\n=== Sending multiple messages ===");
    for i in 1..=3 {
        let (mut send, mut recv) = session.open_bi().await?;
        let msg = format!("Message #{}", i);
        send.write(msg.as_bytes()).await?;
        println!("Sent: {}", msg);
        
        if let Ok(Some(response)) = recv.read().await {
            let text = String::from_utf8_lossy(&response);
            println!("Received: {}", text);
        }
        
        send.finish().await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n=== Testing stream reset ===");
    let (mut send, _recv) = session.open_bi().await?;
    send.write(b"This will be reset").await?;
    send.reset(0);
    println!("Reset stream with code 0");

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\nClient shutting down...");
    Ok(())
}