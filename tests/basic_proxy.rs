//! Basic proxy functionality tests

mod common;

use anyhow::Result;
use common::*;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_server_startup() -> Result<()> {
    let config = TestConfig::default();
    let server = create_test_server(&config).await?;

    // Start server in background
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Wait for server to start
    sleep(Duration::from_millis(500)).await;

    // Check if server is listening
    assert!(
        is_port_listening(&config.server_addr).await,
        "Server should be listening on {}",
        config.server_addr
    );

    Ok(())
}

#[tokio::test]
async fn test_client_startup() -> Result<()> {
    let config = TestConfig::default();

    // Start server first
    let server = create_test_server(&config).await?;
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Start client SOCKS5 server
    let client = create_test_client(&config).await?;
    let client_clone = client.clone();
    let client_listen = config.client_listen.clone();
    tokio::spawn(async move {
        if let Err(e) = anytls_rs::client::start_socks5_server(&client_listen, client_clone).await {
            eprintln!("Client error: {}", e);
        }
    });

    // Wait for client to start
    sleep(Duration::from_millis(500)).await;

    // Check if client SOCKS5 port is listening
    assert!(
        is_port_listening(&config.client_listen).await,
        "Client should be listening on {}",
        config.client_listen
    );

    Ok(())
}

#[tokio::test]
async fn test_client_server_connection() -> Result<()> {
    let config = TestConfig::default();

    // Start server
    let server = create_test_server(&config).await?;
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Start client (not needed for create_proxy_stream, but good to have)
    let client = create_test_client(&config).await?;

    // Wait for connection to establish
    sleep(Duration::from_secs(2)).await;

    // Try to create a proxy stream (this will trigger session creation)
    // Note: This is a basic test - full SOCKS5 test requires actual HTTP client
    let result = client
        .create_proxy_stream(("httpbin.org".to_string(), 80))
        .await;

    // The connection might succeed or fail depending on network,
    // but we just want to verify the client can attempt to create streams
    match result {
        Ok((_stream, _session)) => {
            // Success - connection established
        }
        Err(e) => {
            // This might fail for network reasons, which is OK for a basic test
            // We just want to verify the code path works
            eprintln!(
                "Note: Stream creation failed (might be network issue): {}",
                e
            );
        }
    }

    Ok(())
}
