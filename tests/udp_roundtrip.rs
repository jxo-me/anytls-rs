//! UDP over TCP round-trip integration tests

mod common;

use anyhow::Result;
use bytes::Bytes;
use common::*;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, sleep, timeout};

#[tokio::test]
async fn test_udp_over_tcp_roundtrip() -> Result<()> {
    // Prepare config and start server
    let config = TestConfig::default();
    let server = create_test_server(&config).await?;
    let server_addr = config.server_addr.clone();
    let server_task = tokio::spawn({
        let server_clone = server.clone();
        async move {
            if let Err(e) = server_clone.listen(&server_addr).await {
                tracing::error!("Server error: {}", e);
            }
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // UDP echo server (target)
    let echo_addr: SocketAddr = "127.0.0.1:53530".parse().unwrap();
    let echo_socket = UdpSocket::bind(echo_addr).await?;
    let echo_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        loop {
            match echo_socket.recv_from(&mut buf).await {
                Ok((len, peer)) => {
                    if len == 0 {
                        continue;
                    }
                    let _ = echo_socket.send_to(&buf[..len], peer).await;
                }
                Err(e) => {
                    tracing::error!("Echo server error: {}", e);
                    break;
                }
            }
        }
    });

    // Create client and UDP proxy
    let client = create_test_client(&config).await?;
    let proxy_addr = client.create_udp_proxy("127.0.0.1:0", echo_addr).await?;

    // Allow proxy tasks to initialise
    sleep(Duration::from_millis(200)).await;

    // Simulate local application sending UDP packets
    let app_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let message = b"anytls-udp-test";
    app_socket.send_to(message, proxy_addr).await?;

    let mut buf = vec![0u8; 1024];
    let recv_len = timeout(Duration::from_secs(5), async {
        loop {
            let (len, _) = app_socket.recv_from(&mut buf).await?;
            if len > 0 {
                break Ok::<usize, std::io::Error>(len);
            }
        }
    })
    .await??;

    assert_eq!(
        &buf[..recv_len],
        message,
        "UDP response should match request"
    );

    // Cleanup
    drop(app_socket);
    drop(client);
    // Gracefully stop server & echo task
    server_task.abort();
    echo_task.abort();

    Ok(())
}

#[tokio::test]
async fn test_udp_over_tcp_non_connect_roundtrip() -> Result<()> {
    let config = new_test_config()?;
    let server = create_test_server(&config).await?;
    let server_addr = config.server_addr.clone();
    let server_task = tokio::spawn(async move {
        let _ = server.listen(&server_addr).await;
    });
    sleep(Duration::from_millis(300)).await;

    let echo_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let echo_addr = echo_socket.local_addr()?;
    let echo_task = tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        while let Ok((len, peer)) = echo_socket.recv_from(&mut buf).await {
            let _ = echo_socket.send_to(&buf[..len], peer).await;
        }
    });

    let client = create_test_client(&config).await?;
    let (stream, _session) = client
        .create_proxy_stream(("sp.v2.udp-over-tcp.arpa".to_string(), 0))
        .await?;

    // Request header: isConnect=0 followed by the standard SOCKS address format.
    let mut wire = vec![0, 0x01];
    wire.extend_from_slice(
        &echo_addr
            .ip()
            .to_string()
            .parse::<std::net::Ipv4Addr>()?
            .octets(),
    );
    wire.extend_from_slice(&echo_addr.port().to_be_bytes());
    // Non-connect data packet: UoT address type (IPv4=0), address, port, length, payload.
    wire.push(0x00);
    wire.extend_from_slice(
        &echo_addr
            .ip()
            .to_string()
            .parse::<std::net::Ipv4Addr>()?
            .octets(),
    );
    wire.extend_from_slice(&echo_addr.port().to_be_bytes());
    wire.extend_from_slice(&4u16.to_be_bytes());
    wire.extend_from_slice(b"uot!");
    stream.send_data(Bytes::from(wire)).await?;

    let mut reader = stream.reader().lock().await;
    let mut response = [0u8; 1 + 4 + 2 + 2 + 4];
    timeout(Duration::from_secs(5), reader.read_exact(&mut response)).await??;
    assert_eq!(response[0], 0x00);
    assert_eq!(
        &response[1..5],
        &echo_addr
            .ip()
            .to_string()
            .parse::<std::net::Ipv4Addr>()?
            .octets()
    );
    assert_eq!(
        u16::from_be_bytes([response[5], response[6]]),
        echo_addr.port()
    );
    assert_eq!(u16::from_be_bytes([response[7], response[8]]), 4);
    assert_eq!(&response[9..], b"uot!");

    server_task.abort();
    echo_task.abort();
    Ok(())
}
