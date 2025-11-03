//! Heartbeat mechanism integration tests
//!
//! Tests the HeartRequest/HeartResponse implementation

use anytls_rs::*;
use std::sync::Arc;
use tokio::io::duplex;
use tokio::time::Duration;

#[tokio::test]
async fn test_heartbeat_end_to_end() {
    let _ = tracing_subscriber::fmt::try_init();

    tracing::info!("=== Starting end-to-end heartbeat test ===");

    // Create connected streams
    let (client_stream, server_stream) = duplex(16384);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let padding = PaddingFactory::default();

    // Create sessions
    let client_session = Arc::new(Session::new_client(
        client_read,
        client_write,
        padding.clone(),
    ));

    let server_session = Arc::new(Session::new_server(server_read, server_write, padding));

    // Start background tasks
    let client_clone = client_session.clone();
    tokio::spawn(async move {
        if let Err(e) = client_clone.recv_loop().await {
            tracing::error!("Client recv_loop error: {}", e);
        }
    });

    let server_clone = server_session.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone.recv_loop().await {
            tracing::error!("Server recv_loop error: {}", e);
        }
    });

    let client_clone2 = client_session.clone();
    tokio::spawn(async move {
        if let Err(e) = client_clone2.process_stream_data().await {
            tracing::error!("Client process_stream_data error: {}", e);
        }
    });

    let server_clone2 = server_session.clone();
    tokio::spawn(async move {
        if let Err(e) = server_clone2.process_stream_data().await {
            tracing::error!("Server process_stream_data error: {}", e);
        }
    });

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test 1: Client sends heartbeat request
    tracing::info!("Test 1: Client → Server heartbeat");
    let request = Frame::control(Command::HeartRequest, 0);
    client_session.write_control_frame(request).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Sessions should be alive
    assert!(
        !client_session.is_closed(),
        "Client session should not be closed"
    );
    assert!(
        !server_session.is_closed(),
        "Server session should not be closed"
    );

    // Test 2: Server sends heartbeat request
    tracing::info!("Test 2: Server → Client heartbeat");
    let request = Frame::control(Command::HeartRequest, 1);
    server_session.write_control_frame(request).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Sessions should still be alive
    assert!(!client_session.is_closed());
    assert!(!server_session.is_closed());

    tracing::info!("✅ End-to-end heartbeat test passed");
}

#[tokio::test]
async fn test_heartbeat_stress() {
    let _ = tracing_subscriber::fmt::try_init();

    tracing::info!("=== Starting heartbeat stress test ===");

    let (client_stream, server_stream) = duplex(16384);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let padding = PaddingFactory::default();

    let client_session = Arc::new(Session::new_client(
        client_read,
        client_write,
        padding.clone(),
    ));

    let server_session = Arc::new(Session::new_server(server_read, server_write, padding));

    // Start tasks
    let client_clone = client_session.clone();
    tokio::spawn(async move {
        let _ = client_clone.recv_loop().await;
    });
    let server_clone = server_session.clone();
    tokio::spawn(async move {
        let _ = server_clone.recv_loop().await;
    });
    let client_clone2 = client_session.clone();
    tokio::spawn(async move {
        let _ = client_clone2.process_stream_data().await;
    });
    let server_clone2 = server_session.clone();
    tokio::spawn(async move {
        let _ = server_clone2.process_stream_data().await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send 20 heartbeat requests rapidly
    tracing::info!("Sending 20 rapid heartbeat requests...");
    for i in 0..20 {
        let request = Frame::control(Command::HeartRequest, i);
        client_session.write_control_frame(request).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Wait for all responses to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Sessions should still be alive after stress test
    assert!(
        !client_session.is_closed(),
        "Client session should survive stress test"
    );
    assert!(
        !server_session.is_closed(),
        "Server session should survive stress test"
    );

    tracing::info!("✅ Heartbeat stress test passed (20 requests)");
}

#[tokio::test]
async fn test_heartbeat_with_active_stream() {
    let _ = tracing_subscriber::fmt::try_init();

    tracing::info!("=== Starting heartbeat with active stream test ===");

    let (client_stream, server_stream) = duplex(16384);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let padding = PaddingFactory::default();

    let client_session = Arc::new(Session::new_client(
        client_read,
        client_write,
        padding.clone(),
    ));

    let server_session = Arc::new(Session::new_server(server_read, server_write, padding));

    // Start tasks
    let client_clone = client_session.clone();
    tokio::spawn(async move {
        let _ = client_clone.recv_loop().await;
    });
    let server_clone = server_session.clone();
    tokio::spawn(async move {
        let _ = server_clone.recv_loop().await;
    });
    let client_clone2 = client_session.clone();
    tokio::spawn(async move {
        let _ = client_clone2.process_stream_data().await;
    });
    let server_clone2 = server_session.clone();
    tokio::spawn(async move {
        let _ = server_clone2.process_stream_data().await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Open a stream
    tracing::info!("Opening stream...");
    let (_stream, _synack_rx) = client_session.open_stream().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send heartbeat while stream is active
    tracing::info!("Sending heartbeat while stream is active...");
    let request = Frame::control(Command::HeartRequest, 0);
    client_session.write_control_frame(request).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Everything should still work
    assert!(!client_session.is_closed());
    assert!(!server_session.is_closed());

    tracing::info!("✅ Heartbeat with active stream test passed");
}
