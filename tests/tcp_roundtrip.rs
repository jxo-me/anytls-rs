//! End-to-end TCP roundtrip test through SOCKS5 proxy.
//!
//! This verifies that anytls-server and anytls-client can proxy
//! an HTTP request via SOCKS5 to a local upstream service.

mod common;

use anyhow::Result;
use common::{TestConfig, create_test_client, create_test_server};
use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep, timeout};

fn available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("get local addr")
        .port()
}

#[tokio::test]
async fn test_tcp_roundtrip_via_socks5() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let server_port = available_port();
    let client_port = available_port();
    let http_port = available_port();

    let config = TestConfig {
        server_addr: format!("127.0.0.1:{server_port}"),
        client_listen: format!("127.0.0.1:{client_port}"),
        password: "roundtrip_password".to_string(),
    };

    // Upstream HTTP server that returns a fixed body.
    let http_listener = tokio::net::TcpListener::bind(("127.0.0.1", http_port)).await?;
    let http_task = tokio::spawn(async move {
        if let Ok((mut socket, _)) = http_listener.accept().await {
            let mut buf = vec![0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response =
                b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nHello AnyTLS";
            let _ = socket.write_all(response).await;
            let _ = socket.shutdown().await;
        }
    });

    // Start anytls-server.
    let server = create_test_server(&config).await?;
    let server_clone = Arc::clone(&server);
    let server_addr = config.server_addr.clone();
    let server_task = tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr).await {
            tracing::error!("[Test] Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(300)).await;

    // Start anytls-client + SOCKS5 proxy.
    let client = create_test_client(&config).await?;
    let client_clone = Arc::clone(&client);
    let socks_addr = config.client_listen.clone();
    let socks_addr_clone = socks_addr.clone();
    let socks_task = tokio::spawn(async move {
        if let Err(e) =
            anytls_rs::client::start_socks5_server(&socks_addr_clone, client_clone).await
        {
            tracing::error!("[Test] SOCKS5 error: {}", e);
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Perform SOCKS5 handshake and HTTP request through proxy.
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(&socks_addr)).await??;

    // Greeting: VER, NMETHODS, METHODS
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut method_resp = [0u8; 2];
    stream.read_exact(&mut method_resp).await?;
    assert_eq!(
        method_resp,
        [0x05, 0x00],
        "SOCKS5 method negotiation failed"
    );

    // CONNECT request to upstream HTTP server (IPv4)
    let mut request = vec![0x05, 0x01, 0x00, 0x01];
    request.extend_from_slice(&Ipv4Addr::LOCALHOST.octets());
    request.extend_from_slice(&http_port.to_be_bytes());
    stream.write_all(&request).await?;

    // Response: VER, REP, RSV, ATYP, BND.ADDR, BND.PORT
    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await?;
    assert_eq!(
        reply[1], 0x00,
        "SOCKS5 connect reply error code {}",
        reply[1]
    );

    // Send HTTP request
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;

    // Collect HTTP response
    let mut buf = Vec::new();
    let mut temp = [0u8; 1024];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let remaining = deadline - tokio::time::Instant::now();
        match timeout(remaining, stream.read(&mut temp)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&temp[..n]);
                let response = String::from_utf8_lossy(&buf);
                if response.contains("HTTP/1.1 200 OK") && response.contains("Hello AnyTLS") {
                    break;
                }
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => break,
        }
    }
    let response = String::from_utf8_lossy(&buf);
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "Expected HTTP 200 OK, got: {}",
        response
    );
    assert!(
        response.contains("Hello AnyTLS"),
        "Expected body content missing: {}",
        response
    );
    let _ = stream.shutdown().await;

    // Cleanup
    socks_task.abort();
    let _ = socks_task.await;
    server_task.abort();
    let _ = server_task.await;
    http_task.abort();
    let _ = http_task.await;

    Ok(())
}
