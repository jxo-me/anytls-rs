//! AnyTLS Server binary

use anyhow::{Context, Result};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::server::Server;
use anytls_rs::util::{CertReloader, CertReloaderConfig, StringMap, create_server_config};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "anytls-server";

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments first to get log level
    let mut args = std::env::args().skip(1);
    let mut listen_addr = "0.0.0.0:8443".to_string();
    let mut password = None;
    let mut padding_scheme_file = None;
    let mut cert_path = None;
    let mut key_path = None;
    let mut idle_session_check_interval: Option<u64> = None;
    let mut idle_session_timeout: Option<u64> = None;
    let mut min_idle_session: Option<usize> = None;
    let mut log_level = "info".to_string();
    let mut watch_cert = false;
    let mut show_cert_info = false;
    let mut expiry_warning_days: u64 = 30;

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
            "-L" | "--log-level" => {
                log_level = args
                    .next()
                    .context("Expected log level after --log-level")?;
            }
            "--watch-cert" => {
                watch_cert = true;
            }
            "--show-cert-info" => {
                show_cert_info = true;
            }
            "--expiry-warning-days" => {
                let value = args
                    .next()
                    .context("Expected days after --expiry-warning-days")?;
                expiry_warning_days = parse_u64(&value, "--expiry-warning-days")?;
            }
            "-V" | "--version" => {
                println!("{APP_NAME} {VERSION}");
                return Ok(());
            }
            "-h" | "--help" => {
                println!("Usage: anytls-server [OPTIONS]");
                println!();
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
                println!(
                    "  -L, --log-level LEVEL     Log level: error|warn|info|debug|trace (default: info)"
                );
                println!();
                println!("Certificate Options:");
                println!(
                    "      --watch-cert          Watch certificate files for changes and auto-reload"
                );
                println!("      --show-cert-info      Display certificate information at startup");
                println!(
                    "      --expiry-warning-days DAYS  Certificate expiry warning threshold (default: 30)"
                );
                #[cfg(unix)]
                {
                    println!();
                    println!("Signal Handling:");
                    println!("      SIGHUP                Manually reload TLS certificates");
                }
                println!();
                println!("Other:");
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

    // Initialize tracing with configured log level
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level)),
        )
        .init();

    let password = password.context("Password is required (use -p or --password)")?;

    // Load padding scheme if provided
    let padding = if let Some(file_path) = padding_scheme_file {
        let scheme_bytes = std::fs::read(&file_path)
            .with_context(|| format!("Failed to read padding scheme file: {}", file_path))?;
        let factory = PaddingFactory::new(&scheme_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse padding scheme: {}", e))?;
        info!("[Server] Loaded padding scheme from: {}", file_path);
        Arc::new(factory)
    } else {
        PaddingFactory::default()
    };

    info!("{APP_NAME} v{VERSION}");

    // Create TLS acceptor with optional certificate reloading
    let (tls_acceptor_ref, cert_reloader) = match (cert_path.as_deref(), key_path.as_deref()) {
        (Some(cert), Some(key)) => {
            info!("[Server] Loading TLS certificate from {}", cert);

            // Create certificate reloader
            let config = CertReloaderConfig {
                cert_path: PathBuf::from(cert),
                key_path: PathBuf::from(key),
                watch_enabled: watch_cert,
                debounce_ms: 500,
                check_expiry: true,
                expiry_warning_days,
            };

            let reloader = CertReloader::new(config)
                .with_context(|| format!("Failed to load certificate/key: {cert}, {key}"))?;

            // Show certificate info if requested
            if show_cert_info {
                reloader.show_cert_info();
            }

            let acceptor_ref = reloader.get_acceptor_ref();
            (acceptor_ref, Some(Arc::new(reloader)))
        }
        (None, None) => {
            info!("[Server] No certificate provided, generating self-signed certificate");
            let tls_config =
                create_server_config().context("Failed to create TLS server config")?;
            let tls_acceptor = Arc::new(TlsAcceptor::from(tls_config));
            (Arc::new(std::sync::RwLock::new(tls_acceptor)), None)
        }
        _ => {
            anyhow::bail!("Both --cert and --key must be provided together");
        }
    };

    info!("Listening on {}", listen_addr);

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
    let server =
        Server::new_with_reloadable_tls(&password, tls_acceptor_ref, padding, server_settings);

    // Start certificate file watching if enabled
    if let Some(ref reloader) = cert_reloader
        && watch_cert
    {
        let reloader_clone = reloader.clone();
        if let Err(e) = reloader_clone.start_watching() {
            warn!("[Server] Failed to start certificate file watching: {}", e);
        } else {
            info!("[Server] Certificate file watching enabled");
        }

        // Start expiry checker (check every hour)
        reloader
            .clone()
            .start_expiry_checker(Duration::from_secs(3600));
    }

    // Setup SIGHUP signal handler for manual reload (Unix only)
    #[cfg(unix)]
    if let Some(reloader) = cert_reloader.clone() {
        tokio::spawn(async move {
            let mut sighup = match signal(SignalKind::hangup()) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[Server] Failed to setup SIGHUP handler: {}", e);
                    return;
                }
            };

            info!("[Server] SIGHUP handler ready (send SIGHUP to reload certificates)");

            loop {
                sighup.recv().await;
                info!("[Server] SIGHUP received, reloading certificates...");

                match reloader.reload() {
                    Ok(_) => {
                        info!("[Server] Certificate reload successful");
                        if let Some(info) = reloader.get_cert_info() {
                            info!("[Server] New certificate: {}", info.summary());
                        }
                    }
                    Err(e) => {
                        error!("[Server] Certificate reload failed: {}", e);
                    }
                }
            }
        });
    }

    #[cfg(not(unix))]
    if cert_reloader.is_some() {
        info!("[Server] Note: SIGHUP signal reload not available on Windows. Use --watch-cert for automatic reload.");
    }

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
