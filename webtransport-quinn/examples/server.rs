use anyhow::Context;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "[::]:4443")]
    addr: std::net::SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Enable info logging.
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Args::parse();

    // Generate a self-signed certificate
    let gen = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();

    // Convert a rcgen Certificate to a rustls Certificate
    let cert = rustls::Certificate(gen.serialize_der().unwrap());
    let key = rustls::PrivateKey(gen.serialize_private_key_der());

    // Standard Quinn setup
    let mut tls_config = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;

    tls_config.max_early_data_size = u32::MAX;
    tls_config.alpn_protocols = vec![webtransport_quinn::ALPN.to_vec()]; // this one is important

    let config = quinn::ServerConfig::with_crypto(std::sync::Arc::new(tls_config));

    log::info!("listening on {}", args.addr);

    let server = quinn::Endpoint::server(config, args.addr)?;

    // Accept new connections.
    while let Some(conn) = server.accept().await {
        tokio::spawn(async move {
            let err = run_conn(conn).await;
            if let Err(err) = err {
                log::error!("connection failed: {}", err)
            }
        });
    }

    // TODO simple echo server

    Ok(())
}

async fn run_conn(conn: quinn::Connecting) -> anyhow::Result<()> {
    log::info!("received new QUIC connection");

    // Wait for the QUIC handshake to complete.
    let conn = conn.await.context("failed to accept connection")?;
    log::info!("established QUIC connection");

    // Perform the WebTransport handshake.
    let request = webtransport_quinn::accept(conn).await?;
    log::info!("received WebTransport request: {}", request.uri());

    // Parse the request URI to decide if we should accept the session.
    let (initial, count) = match webtransport_baton::parse(request.uri()) {
        Ok(v) => v,
        Err(err) => {
            log::info!("invalid request: {}", err);

            // Reject the session.
            request.close(http::StatusCode::BAD_REQUEST).await?;
            return Err(err);
        }
    };

    // Accept the session.
    let session = request.ok().await.context("failed to accept session")?;
    log::info!("accepted session");

    // Run the baton code, creating the initial batons.
    webtransport_baton::run(session, Some(initial), count).await?;

    log::info!("finished baton successfully!");

    Ok(())
}
