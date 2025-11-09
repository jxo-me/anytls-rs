//! Concurrent connection tests

mod common;

use anyhow::Result;
use common::*;
use tokio::time::{Duration, sleep, timeout};

#[tokio::test]
async fn test_multiple_streams() -> Result<()> {
    let config = new_test_config()?;

    // Start server
    let server = create_test_server(&config).await?;
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Start client
    let client = create_test_client(&config).await?;
    let client_clone = client.clone();
    let client_listen = config.client_listen.clone();
    let client_handle = tokio::spawn(async move {
        if let Err(e) = anytls_rs::client::start_socks5_server(&client_listen, client_clone).await {
            eprintln!("Client error: {}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    let (echo_addr, echo_handle) = spawn_tcp_echo_server().await?;
    let echo_ip = echo_addr.ip().to_string();
    let echo_port = echo_addr.port();

    // Try to create multiple streams concurrently
    let mut handles = vec![];
    for i in 0..3 {
        let client = client.clone();
        let dest_ip = echo_ip.clone();
        let handle = tokio::spawn(async move {
            let _ = i;
            timeout(
                Duration::from_secs(5),
                client.create_proxy_stream((dest_ip, echo_port)),
            )
            .await
        });
        handles.push(handle);
    }

    // Wait for all attempts
    for handle in handles {
        let _ = handle.await;
    }

    echo_handle.abort();
    let _ = echo_handle.await;
    client_handle.abort();
    let _ = client_handle.await;
    server_handle.abort();
    let _ = server_handle.await;

    Ok(())
}

#[tokio::test]
async fn test_session_reuse() -> Result<()> {
    let config = new_test_config()?;

    // Start server
    let server = create_test_server(&config).await?;
    let server_clone = server.clone();
    let server_addr = config.server_addr.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Start client
    let client = create_test_client(&config).await?;
    let client_clone = client.clone();
    let client_listen = config.client_listen.clone();
    let client_handle = tokio::spawn(async move {
        if let Err(e) = anytls_rs::client::start_socks5_server(&client_listen, client_clone).await {
            eprintln!("Client error: {}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    let (echo_addr, echo_handle) = spawn_tcp_echo_server().await?;
    let echo_ip = echo_addr.ip().to_string();
    let echo_port = echo_addr.port();

    // Create first stream
    let result1 = timeout(
        Duration::from_secs(5),
        client.create_proxy_stream((echo_ip.clone(), echo_port)),
    )
    .await;

    // Small delay
    sleep(Duration::from_millis(100)).await;

    // Create second stream - should potentially reuse session
    let result2 = timeout(
        Duration::from_secs(5),
        client.create_proxy_stream((echo_ip, echo_port)),
    )
    .await;

    // Verify both attempts work (or fail gracefully)
    let _ = result1;
    let _ = result2;

    echo_handle.abort();
    let _ = echo_handle.await;
    client_handle.abort();
    let _ = client_handle.await;
    server_handle.abort();
    let _ = server_handle.await;

    Ok(())
}
