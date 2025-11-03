//! AnyTLS Server binary

use anyhow::{Context, Result};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::server::Server;
use anytls_rs::util::create_server_config;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Parse command line arguments
    let mut args = std::env::args().skip(1);
    let mut listen_addr = "0.0.0.0:8443".to_string();
    let mut password = None;
    let mut padding_scheme_file = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-l" | "--listen" => {
                listen_addr = args.next().context("Expected listen address after -l")?;
            }
            "-p" | "--password" => {
                password = Some(args.next().context("Expected password after -p")?);
            }
            "--padding-scheme" => {
                padding_scheme_file = Some(args.next().context("Expected padding scheme file after --padding-scheme")?);
            }
            "-h" | "--help" => {
                println!("Usage: anytls-server [OPTIONS]");
                println!("Options:");
                println!("  -l, --listen ADDRESS      Listen address (default: 0.0.0.0:8443)");
                println!("  -p, --password PASSWORD    Server password (required)");
                println!("  --padding-scheme FILE      Path to padding scheme file");
                println!("  -h, --help                Show this help message");
                return Ok(());
            }
            _ => {
                error!("Unknown argument: {}", arg);
                return Err(anyhow::anyhow!("Unknown argument: {}", arg));
            }
        }
    }

    let password = password.context("Password is required (use -p or --password)")?;

    // Load padding scheme if provided
    let padding = if let Some(file_path) = padding_scheme_file {
        let scheme_bytes = std::fs::read(&file_path)
            .with_context(|| format!("Failed to read padding scheme file: {}", file_path))?;
        let factory = PaddingFactory::new(&scheme_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse padding scheme: {}", e))?;
        info!("Loaded padding scheme from: {}", file_path);
        Arc::new(factory)
    } else {
        PaddingFactory::default()
    };

    // Create TLS config
    let tls_config = create_server_config()
        .context("Failed to create TLS server config")?;
    let tls_acceptor = TlsAcceptor::from(tls_config);

    info!("[Server] anytls-rs v0.1.0");
    info!("[Server] Listening TCP {}", listen_addr);

    // Create and start server
    let server = Server::new(
        &password,
        Arc::new(tls_acceptor),
        padding,
    );

    // Start listening
    server.listen(&listen_addr).await
        .context("Failed to start server")?;

    Ok(())
}
