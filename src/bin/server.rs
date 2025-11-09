//! AnyTLS Server binary

use anyhow::{Context, Result};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::server::Server;
use anytls_rs::util::{StringMap, create_server_config, create_server_config_from_files};
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "anytls-server";

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
    let mut cert_path = None;
    let mut key_path = None;
    let mut idle_session_check_interval: Option<u64> = None;
    let mut idle_session_timeout: Option<u64> = None;
    let mut min_idle_session: Option<usize> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-l" | "--listen" => {
                listen_addr = args.next().context("Expected listen address after -l")?;
            }
            "-p" | "--password" => {
                password = Some(args.next().context("Expected password after -p")?);
            }
            "--padding-scheme" => {
                padding_scheme_file = Some(
                    args.next()
                        .context("Expected padding scheme file after --padding-scheme")?,
                );
            }
            "--cert" => {
                cert_path = Some(
                    args.next()
                        .context("Expected certificate path after --cert")?,
                );
            }
            "--key" => {
                key_path = Some(
                    args.next()
                        .context("Expected private key path after --key")?,
                );
            }
            "-I" | "--idle-session-check-interval" => {
                let value = args
                    .next()
                    .context("Expected seconds after --idle-session-check-interval")?;
                idle_session_check_interval =
                    Some(parse_u64(&value, "--idle-session-check-interval")?);
            }
            "-T" | "--idle-session-timeout" => {
                let value = args
                    .next()
                    .context("Expected seconds after --idle-session-timeout")?;
                idle_session_timeout = Some(parse_u64(&value, "--idle-session-timeout")?);
            }
            "-M" | "--min-idle-session" => {
                let value = args
                    .next()
                    .context("Expected value after --min-idle-session")?;
                min_idle_session = Some(parse_usize(&value, "--min-idle-session")?);
            }
            "-V" | "--version" => {
                println!("{APP_NAME} {VERSION}");
                return Ok(());
            }
            "-h" | "--help" => {
                println!("Usage: anytls-server [OPTIONS]");
                println!("Options:");
                println!("  -l, --listen ADDRESS      Listen address (default: 0.0.0.0:8443)");
                println!("  -p, --password PASSWORD    Server password (required)");
                println!("      --cert FILE            Path to PEM encoded TLS certificate");
                println!("      --key  FILE            Path to PEM encoded TLS private key");
                println!("      --padding-scheme FILE  Path to padding scheme file");
                println!(
                    "  -I, --idle-session-check-interval SECS  Hint for clients (default: 30)"
                );
                println!(
                    "  -T, --idle-session-timeout SECS         Hint for clients (default: 60)"
                );
                println!("  -M, --min-idle-session COUNT            Hint for clients (default: 1)");
                println!("  -V, --version             Show version information");
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
    let tls_config = match (cert_path.as_deref(), key_path.as_deref()) {
        (Some(cert), Some(key)) => {
            info!("[Server] Loading TLS certificate from {}", cert);
            create_server_config_from_files(cert, key)
                .with_context(|| format!("Failed to load certificate/key: {cert}, {key}"))?
        }
        (None, None) => {
            info!("[Server] No certificate provided, generating self-signed certificate");
            create_server_config().context("Failed to create TLS server config")?
        }
        _ => {
            anyhow::bail!("Both --cert and --key must be provided together");
        }
    };
    let tls_acceptor = TlsAcceptor::from(tls_config);

    info!("[Server] {APP_NAME} v{VERSION}");
    info!("[Server] Listening TCP {}", listen_addr);

    let mut server_settings_map = StringMap::new();
    if let Some(interval) = idle_session_check_interval {
        server_settings_map.insert("idle_session_check_interval", interval.to_string());
    }
    if let Some(timeout) = idle_session_timeout {
        server_settings_map.insert("idle_session_timeout", timeout.to_string());
    }
    if let Some(min_idle) = min_idle_session {
        server_settings_map.insert("min_idle_session", min_idle.to_string());
    }
    let server_settings = if server_settings_map.is_empty() {
        None
    } else {
        Some(server_settings_map)
    };

    // Create and start server
    let server = Server::new(&password, Arc::new(tls_acceptor), padding, server_settings);

    // Start listening
    server
        .listen(&listen_addr)
        .await
        .context("Failed to start server")?;

    Ok(())
}

fn parse_u64(value: &str, flag: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("{} expects a positive integer: {}", flag, e))?;
    if parsed == 0 {
        anyhow::bail!("{} expects a value greater than 0", flag);
    }
    Ok(parsed)
}

fn parse_usize(value: &str, flag: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .map_err(|e| anyhow::anyhow!("{} expects a non-negative integer: {}", flag, e))
}
