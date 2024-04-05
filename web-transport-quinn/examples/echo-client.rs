use std::{fs, io, path};

use anyhow::Context;
use clap::Parser;
use rustls::Certificate;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "https://localhost:4443")]
    url: Url,

    /// Accept the certificates at this path, encoded as PEM.
    #[arg(long)]
    pub tls_cert: path::PathBuf,
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

    let chain: Vec<Certificate> = rustls_pemfile::certs(&mut chain)?
        .into_iter()
        .map(Certificate)
        .collect();

    anyhow::ensure!(!chain.is_empty(), "could not find certificate");

    let mut roots = rustls::RootCertStore::empty();
    roots.add(&chain[0])?;

    // Standard quinn setup, accepting only the given certificate.
    // You should use system roots in production.
    let mut tls_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();

    tls_config.alpn_protocols = vec![web_transport_quinn::ALPN.to_vec()]; // this one is important

    let config = quinn::ClientConfig::new(std::sync::Arc::new(tls_config));

    let addr = "[::]:0".parse()?;
    let mut client = quinn::Endpoint::client(addr)?;
    client.set_default_client_config(config);

    log::info!("connecting to {}", args.url);

    // Connect to the given URL.
    let session = web_transport_quinn::connect(&client, &args.url).await?;

    log::info!("connected");

    // Create a bidirectional stream.
    let (mut send, mut recv) = session.open_bi().await?;

    log::info!("created stream");

    // Send a message.
    let msg = "hello world".to_string();
    send.write_all(msg.as_bytes()).await?;
    log::info!("sent: {}", msg);

    // Shut down the send stream.
    send.finish().await?;

    // Read back the message.
    let msg = recv.read_to_end(1024).await?;
    log::info!("recv: {}", String::from_utf8_lossy(&msg));

    Ok(())
}
