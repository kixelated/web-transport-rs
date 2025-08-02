use std::{
    fs,
    io::{self, Read},
    path,
    sync::Arc,
};

use anyhow::Context;

use clap::Parser;
use rustls::pki_types::CertificateDer;
use web_transport_quinn::Session;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "[::]:4443")]
    addr: std::net::SocketAddr,

    /// Use the certificates at this path, encoded as PEM.
    #[arg(long)]
    pub tls_cert: path::PathBuf,

    /// Use the private key at this path, encoded as PEM.
    #[arg(long)]
    pub tls_key: path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Enable info logging.
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Args::parse();

    // Read the PEM certificate chain
    let chain = fs::File::open(args.tls_cert).context("failed to open cert file")?;
    let mut chain = io::BufReader::new(chain);

    let chain: Vec<CertificateDer> = rustls_pemfile::certs(&mut chain)
        .collect::<Result<_, _>>()
        .context("failed to load certs")?;

    anyhow::ensure!(!chain.is_empty(), "could not find certificate");

    // Read the PEM private key
    let mut keys = fs::File::open(args.tls_key).context("failed to open key file")?;

    // Read the keys into a Vec so we can parse it twice.
    let mut buf = Vec::new();
    keys.read_to_end(&mut buf)?;

    // Try to parse a PKCS#8 key
    // -----BEGIN PRIVATE KEY-----
    let key = rustls_pemfile::private_key(&mut io::Cursor::new(&buf))
        .context("failed to load private key")?
        .context("missing private key")?;

    // Standard Quinn setup
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(chain, key)?;

    config.max_early_data_size = u32::MAX;
    config.alpn_protocols = vec![web_transport_quinn::ALPN.as_bytes().to_vec()]; // this one is important

    let config: quinn::crypto::rustls::QuicServerConfig = config.try_into()?;
    let config = quinn::ServerConfig::with_crypto(Arc::new(config));

    log::info!("listening on {}", args.addr);

    let server = quinn::Endpoint::server(config, args.addr)?;

    // Accept new connections.
    while let Some(conn) = server.accept().await {
        tokio::spawn(async move {
            let err = run_conn(conn).await;
            if let Err(err) = err {
                log::error!("connection failed: {err}")
            }
        });
    }

    // TODO simple echo server

    Ok(())
}

async fn run_conn(conn: quinn::Incoming) -> anyhow::Result<()> {
    log::info!("received new QUIC connection");

    // Wait for the QUIC handshake to complete.
    let conn = conn.await.context("failed to accept connection")?;
    log::info!("established QUIC connection");

    // Perform the WebTransport handshake.
    let request = web_transport_quinn::Request::accept(conn).await?;
    log::info!("received WebTransport request: {}", request.url());

    // Accept the session.
    let session = request.ok().await.context("failed to accept session")?;
    log::info!("accepted session");

    // Run the session
    if let Err(err) = run_session(session).await {
        log::info!("closing session: {err}");
    }

    Ok(())
}

async fn run_session(session: Session) -> anyhow::Result<()> {
    loop {
        // Wait for a bidirectional stream or datagram.
        tokio::select! {
            res = session.accept_bi() => {
                let (mut send, mut recv) = res?;
                log::info!("accepted stream");

                // Read the message and echo it back.
                let msg = recv.read_to_end(1024).await?;
                log::info!("recv: {}", String::from_utf8_lossy(&msg));

                send.write_all(&msg).await?;
                log::info!("send: {}", String::from_utf8_lossy(&msg));
            },
            res = session.read_datagram() => {
                let msg = res?;
                log::info!("accepted datagram");
                log::info!("recv: {}", String::from_utf8_lossy(&msg));

                session.send_datagram(msg.clone())?;
                log::info!("send: {}", String::from_utf8_lossy(&msg));
            },
        };

        log::info!("echo successful!");
    }
}
