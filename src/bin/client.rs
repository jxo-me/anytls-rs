//! AnyTLS Client binary

use anyhow::{Context, Result};
use anytls_rs::client::{start_socks5_server, Client};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::util::create_client_config;
use std::sync::Arc;
use tokio_rustls::TlsConnector;
use tracing::{error, info};

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
    let mut listen_addr = "127.0.0.1:1080".to_string();
    let mut server_addr = "127.0.0.1:8443".to_string();
    let mut sni = None;
    let mut password = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-l" | "--listen" => {
                listen_addr = args.next().context("Expected listen address after -l")?;
            }
            "-s" | "--server" => {
                server_addr = args.next().context("Expected server address after -s")?;
            }
            "--sni" => {
                sni = Some(args.next().context("Expected SNI after --sni")?);
            }
            "-p" | "--password" => {
                password = Some(args.next().context("Expected password after -p")?);
            }
            "-h" | "--help" => {
                println!("Usage: anytls-client [OPTIONS]");
                println!("Options:");
                println!(
                    "  -l, --listen ADDRESS      SOCKS5 listen address (default: 127.0.0.1:1080)"
                );
                println!("  -s, --server ADDRESS     Server address (default: 127.0.0.1:8443)");
                println!("  --sni SNI                 TLS SNI (optional)");
                println!("  -p, --password PASSWORD  Server password (required)");
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

    // Create TLS config
    let client_config =
        create_client_config(sni.clone()).context("Failed to create TLS client config")?;

    // Create TLS connector
    let tls_connector = TlsConnector::from(client_config);

    // Create padding factory
    let padding = PaddingFactory::default();

    info!("[Client] anytls-rs v0.1.0");
    info!("[Client] SOCKS5/HTTP {} => {}", listen_addr, server_addr);

    // Create client
    let client = Arc::new(Client::new(
        &password,
        server_addr,
        Arc::new(tls_connector),
        padding,
    ));

    info!("[Client] Client created successfully");

    // Start SOCKS5 server
    start_socks5_server(&listen_addr, client)
        .await
        .context("SOCKS5 server error")?;

    Ok(())
}
