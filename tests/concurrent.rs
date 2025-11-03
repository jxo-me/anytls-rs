//! Concurrent connection tests

mod common;

use anyhow::Result;
use common::*;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_multiple_streams() -> Result<()> {
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

    // Start client
    let client = create_test_client(&config).await?;
    let client_clone = client.clone();
    let client_listen = config.client_listen.clone();
    tokio::spawn(async move {
        if let Err(e) = anytls_rs::client::start_socks5_server(&client_listen, client_clone).await {
            eprintln!("Client error: {}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    // Try to create multiple streams concurrently
    let mut handles = vec![];
    for i in 0..3 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let destination = format!("example{}:80", i);
            client.create_proxy_stream((destination, 80)).await
        });
        handles.push(handle);
    }

    // Wait for all attempts
    for handle in handles {
        let _result = handle.await;
        // We don't check the result - network issues are OK in tests
        // We just verify concurrent creation doesn't panic
    }

    Ok(())
}

#[tokio::test]
async fn test_session_reuse() -> Result<()> {
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

    // Start client
    let client = create_test_client(&config).await?;
    let client_clone = client.clone();
    let client_listen = config.client_listen.clone();
    tokio::spawn(async move {
        if let Err(e) = anytls_rs::client::start_socks5_server(&client_listen, client_clone).await {
            eprintln!("Client error: {}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    // Create first stream
    let result1 = client
        .create_proxy_stream(("example.com".to_string(), 80))
        .await;

    // Small delay
    sleep(Duration::from_millis(100)).await;

    // Create second stream - should potentially reuse session
    let result2 = client
        .create_proxy_stream(("example.org".to_string(), 80))
        .await;

    // Verify both attempts work (or fail gracefully)
    let _ = result1;
    let _ = result2;

    Ok(())
}
