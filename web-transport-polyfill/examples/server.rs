use tokio_tungstenite::accept_async;
use tokio::net::TcpListener;
use web_transport_polyfill::Session;
use web_transport_generic::{Session as _, RecvStream, SendStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:3000";
    let listener = TcpListener::bind(addr).await?;
    println!("WebSocket server listening on ws://{}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        println!("New connection from: {}", addr);
        
        tokio::spawn(async move {
            let ws_stream = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    println!("WebSocket handshake failed: {}", e);
                    return;
                }
            };
            
            println!("WebSocket connection established with {}", addr);
            let session = Session::new(ws_stream, true);
            
            // Spawn tasks to handle different stream types
            let addr_clone = addr.clone();
            let mut uni_session = session.clone();
            let uni_handler = tokio::spawn(async move {
                loop {
                    match uni_session.accept_uni().await {
                        Ok(mut stream) => {
                            println!("Accepted unidirectional stream from {}", addr_clone);
                            let addr_inner = addr_clone.clone();
                            tokio::spawn(async move {
                                while let Ok(Some(data)) = stream.read().await {
                                    println!("[{}] Received on uni stream: {} bytes", addr_inner, data.len());
                                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                                        println!("[{}] Message: {}", addr_inner, text);
                                    }
                                }
                                println!("[{}] Uni stream closed", addr_inner);
                            });
                        }
                        Err(e) => {
                            println!("Error accepting uni stream: {}", e);
                            break;
                        }
                    }
                }
            });

            let addr_clone = addr.clone();
            let mut bi_session = session.clone();
            let bi_handler = tokio::spawn(async move {
                loop {
                    match bi_session.accept_bi().await {
                        Ok((mut send, mut recv)) => {
                            println!("Accepted bidirectional stream from {}", addr_clone);
                            let addr_inner = addr_clone.clone();
                            tokio::spawn(async move {
                                while let Ok(Some(data)) = recv.read().await {
                                    println!("[{}] Received on bi stream: {} bytes", addr_inner, data.len());
                                    if let Ok(text) = String::from_utf8(data.to_vec()) {
                                        println!("[{}] Echo message: {}", addr_inner, text);
                                        let response = format!("Echo: {}", text);
                                        if let Err(e) = send.write(response.as_bytes()).await {
                                            println!("Error sending response: {}", e);
                                            break;
                                        }
                                    }
                                }
                                let _ = send.finish().await;
                                println!("[{}] Bi stream closed", addr_inner);
                            });
                        }
                        Err(e) => {
                            println!("Error accepting bi stream: {}", e);
                            break;
                        }
                    }
                }
            });

            // Wait for either handler to finish
            tokio::select! {
                _ = uni_handler => {},
                _ = bi_handler => {},
            }
            
            println!("Session with {} ended", addr);
        });
    }

    Ok(())
}