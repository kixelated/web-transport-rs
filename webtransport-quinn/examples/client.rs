use clap::Parser;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "https://localhost:4443/webtransport/devious-baton"
    )]
    url: Url,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Enable info logging.
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Args::parse();

    // Standard quinn setup.
    let mut tls_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new()) // WARNING: Don't use this in production
        .with_no_client_auth();

    tls_config.alpn_protocols = vec![webtransport_quinn::ALPN.to_vec()]; // this one is important

    let config = quinn::ClientConfig::new(std::sync::Arc::new(tls_config));

    let addr = "[::]:0".parse()?;
    let mut client = quinn::Endpoint::client(addr)?;
    client.set_default_client_config(config);

    //	Create the WebTransport URL.
    let batons = 1;

    log::info!("connecting to {}", args.url);

    // Connect to the given URL.
    let session = webtransport_quinn::connect(&client, &args.url).await?;

    // Run the baton code.
    webtransport_baton::run(session, None, batons).await?;

    log::info!("finished baton successfully!");

    Ok(())
}

// Implementation of `ServerCertVerifier` that verifies everything as trustworthy.
// WARNING: Don't use this in production.
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
