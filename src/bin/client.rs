//! AnyTLS Client binary

use anyhow::{Context, Result};
use anytls_rs::client::{Client, SessionPoolConfig, start_http_proxy_server, start_socks5_server};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::util::create_client_config;
use std::sync::Arc;
use std::time::Duration;
use tokio_rustls::TlsConnector;
use tracing::{error, info};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "anytls-client";

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
    let mut http_listen_addr: Option<String> = None;
    let mut server_addr = "127.0.0.1:8443".to_string();
    let mut sni = None;
    let mut password = None;
    let mut idle_check_interval: Option<u64> = None;
    let mut idle_timeout: Option<u64> = None;
    let mut min_idle_sessions: Option<usize> = None;

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
            "-H" | "--http-listen" => {
                http_listen_addr = Some(
                    args.next()
                        .context("Expected listen address after --http-listen")?,
                );
            }
            "-I" | "--idle-session-check-interval" => {
                let value = args
                    .next()
                    .context("Expected seconds after --idle-session-check-interval")?;
                idle_check_interval = Some(parse_u64(&value, "--idle-session-check-interval")?);
            }
            "-T" | "--idle-session-timeout" => {
                let value = args
                    .next()
                    .context("Expected seconds after --idle-session-timeout")?;
                idle_timeout = Some(parse_u64(&value, "--idle-session-timeout")?);
            }
            "-M" | "--min-idle-session" => {
                let value = args
                    .next()
                    .context("Expected value after --min-idle-session")?;
                min_idle_sessions = Some(parse_usize(&value, "--min-idle-session")?);
            }
            "-V" | "--version" => {
                println!("{APP_NAME} {VERSION}");
                return Ok(());
            }
            "-h" | "--help" => {
                println!("Usage: anytls-client [OPTIONS]");
                println!("Options:");
                println!(
                    "  -l, --listen ADDRESS      SOCKS5 listen address (default: 127.0.0.1:1080)"
                );
                println!("  -s, --server ADDRESS     Server address (default: 127.0.0.1:8443)");
                println!("  --sni SNI                 TLS SNI (optional)");
                println!("  -H, --http-listen ADDRESS  HTTP proxy listen address (optional)");
                println!(
                    "  -I, --idle-session-check-interval SECS  Idle session check interval (default: 30)"
                );
                println!(
                    "  -T, --idle-session-timeout SECS         Idle session timeout (default: 60)"
                );
                println!(
                    "  -M, --min-idle-session COUNT            Minimum idle sessions retained (default: 1)"
                );
                println!("  -V, --version             Show version information");
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

    // Create padding factory & session pool config
    let padding = PaddingFactory::default();
    let mut pool_config = SessionPoolConfig::default();
    if let Some(secs) = idle_check_interval {
        pool_config.check_interval = Duration::from_secs(secs);
    }
    if let Some(secs) = idle_timeout {
        pool_config.idle_timeout = Duration::from_secs(secs);
    }
    if let Some(count) = min_idle_sessions {
        pool_config.min_idle_sessions = count;
    }

    info!("[Client] {APP_NAME} v{VERSION}");
    if let Some(http_addr) = http_listen_addr.as_ref() {
        info!(
            "[Client] SOCKS5 {} + HTTP {} => {}",
            listen_addr, http_addr, server_addr
        );
    } else {
        info!("[Client] SOCKS5 {} => {}", listen_addr, server_addr);
    }

    // Create client
    let client = Arc::new(Client::with_pool_config(
        &password,
        server_addr,
        Arc::new(tls_connector),
        padding,
        pool_config,
    ));

    info!("[Client] Client created successfully");

    // Start proxy servers
    if let Some(http_addr) = http_listen_addr {
        let socks_addr = listen_addr.clone();
        let socks_client = Arc::clone(&client);
        let http_client = Arc::clone(&client);

        let socks_task = tokio::spawn(async move {
            start_socks5_server(&socks_addr, socks_client)
                .await
                .context("SOCKS5 server error")
        });
        let http_task = tokio::spawn(async move {
            start_http_proxy_server(&http_addr, http_client)
                .await
                .context("HTTP proxy server error")
        });

        tokio::select! {
            res = socks_task => res.context("SOCKS5 task join error")??,
            res = http_task => res.context("HTTP task join error")??,
        }
    } else {
        start_socks5_server(&listen_addr, client)
            .await
            .context("SOCKS5 server error")?;
    }

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
