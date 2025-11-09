use anyhow::{Result, bail};
use tokio::time::{Duration, sleep, timeout};

mod common;
use common::{
    create_test_client_with_config, create_test_server, is_port_listening, new_test_config,
    spawn_tcp_echo_server, wait_for,
};

#[tokio::test]
async fn test_client_recovers_after_server_restart() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let config = new_test_config()?;

    // Start initial server
    let server = create_test_server(&config).await?;
    let server_addr = config.server_addr.clone();
    let server_handle = tokio::spawn({
        let server = server.clone();
        async move {
            if let Err(e) = server.listen(&server_addr).await {
                eprintln!("Server error: {}", e);
            }
        }
    });

    // Use aggressive heartbeat/timeout to detect disconnects quickly
    let pool_config = anytls_rs::client::SessionPoolConfig {
        check_interval: Duration::from_millis(300),
        idle_timeout: Duration::from_secs(1),
        min_idle_sessions: 0,
    };
    let client = create_test_client_with_config(&config, pool_config.clone()).await?;
    client.stop_session_pool_cleanup().await;

    // Prepare upstream echo server
    let (echo_addr, echo_handle) = spawn_tcp_echo_server().await?;
    let echo_ip = echo_addr.ip().to_string();
    let echo_port = echo_addr.port();

    sleep(Duration::from_millis(500)).await;

    // Establish first stream/session
    let first_attempt = timeout(
        Duration::from_secs(10),
        client.create_proxy_stream((echo_ip.clone(), echo_port)),
    )
    .await;
    let (stream1, initial_session) = match first_attempt {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => bail!("initial stream creation timed out"),
    };

    let initial_session_id = initial_session.id();

    // Simulate server restart: stop listener and close existing session
    server_handle.abort();
    let _ = server_handle.await;
    drop(stream1);
    initial_session.close().await?;

    // Start replacement server on the same address
    let server = create_test_server(&config).await?;
    let server_addr = config.server_addr.clone();
    let server_handle = tokio::spawn({
        let server = server.clone();
        async move {
            if let Err(e) = server.listen(&server_addr).await {
                eprintln!("Server error: {}", e);
            }
        }
    });

    // Wait for client session to detect closure via heartbeat
    let session_closed = wait_for(|| initial_session.is_closed(), Duration::from_secs(5)).await;
    assert!(
        session_closed,
        "initial session should close after server shutdown"
    );

    sleep(Duration::from_millis(500)).await;
    drop(initial_session);
    client.stop_session_pool_cleanup().await;
    drop(client);

    // Recreate client to avoid retaining stale sessions
    let client = create_test_client_with_config(&config, pool_config.clone()).await?;
    client.stop_session_pool_cleanup().await;

    // Confirm new server is accepting connections
    for _ in 0..20 {
        if is_port_listening(&config.server_addr).await {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    // Establish new stream/session and ensure client recovers
    let second_attempt = timeout(
        Duration::from_secs(10),
        client.create_proxy_stream((echo_ip, echo_port)),
    )
    .await;
    let (stream2, new_session) = match second_attempt {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => bail!("reconnection stream creation timed out"),
    };

    assert_ne!(
        initial_session_id,
        new_session.id(),
        "client should create a fresh session after reconnect"
    );

    // Cleanup
    drop(stream2);
    client.stop_session_pool_cleanup().await;
    drop(client);

    server_handle.abort();
    let _ = server_handle.await;
    echo_handle.abort();
    let _ = echo_handle.await;

    Ok(())
}

#[tokio::test]
async fn test_session_shutdown_closes_background_tasks() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let config = new_test_config()?;
    let server = create_test_server(&config).await?;
    let server_addr = config.server_addr.clone();

    let server_handle = tokio::spawn({
        let server = server.clone();
        async move {
            if let Err(e) = server.listen(&server_addr).await {
                eprintln!("Server error: {}", e);
            }
        }
    });

    let client = create_test_client_with_config(
        &config,
        anytls_rs::client::SessionPoolConfig {
            check_interval: Duration::from_millis(300),
            idle_timeout: Duration::from_secs(1),
            min_idle_sessions: 0,
        },
    )
    .await?;

    let (echo_addr, echo_handle) = spawn_tcp_echo_server().await?;
    sleep(Duration::from_millis(300)).await;

    let (stream, session) = client
        .create_proxy_stream((echo_addr.ip().to_string(), echo_addr.port()))
        .await?;

    drop(stream);
    session.close().await?;

    let closed = wait_for(|| session.is_closed(), Duration::from_secs(3)).await;
    assert!(closed, "session should close within timeout");

    client.stop_session_pool_cleanup().await;
    drop(client);

    server_handle.abort();
    let _ = server_handle.await;
    echo_handle.abort();
    let _ = echo_handle.await;

    Ok(())
}
