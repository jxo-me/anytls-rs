//! End-to-end performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench e2e_bench
//!
//! These benchmarks measure complete data flow through client and server sessions

use anyhow::Result;
use anytls_rs::client::Client;
use anytls_rs::padding::PaddingFactory;
use anytls_rs::server::Server;
use anytls_rs::session::Session;
use anytls_rs::util::tls;
use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::net::TcpListener;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UdpSocket;
use tokio::time::{Duration, sleep, timeout};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Mock async reader/writer for testing (not used, but kept for future use)
/// Currently using tokio::io::duplex for connected sessions
struct MockStream {
    // Reserved for future implementation
}

impl MockStream {
    #[allow(dead_code)]
    fn new() -> Self {
        Self {}
    }
}

// MockStream implementation removed - using tokio::io::duplex instead

/// Create a client-server session pair with connected streams
async fn create_connected_sessions() -> (Arc<Session>, Arc<Session>) {
    let (client_read, server_write) = tokio::io::duplex(65536);
    let (server_read, client_write) = tokio::io::duplex(65536);

    let padding = PaddingFactory::default();

    let client_session = Arc::new(Session::new_client(
        Box::new(client_read) as Box<dyn AsyncRead + Send + Unpin>,
        Box::new(client_write) as Box<dyn AsyncWrite + Send + Unpin>,
        padding.clone(),
        None,
    ));

    let server_session = Arc::new(Session::new_server(
        Box::new(server_read) as Box<dyn AsyncRead + Send + Unpin>,
        Box::new(server_write) as Box<dyn AsyncWrite + Send + Unpin>,
        padding,
    ));

    (client_session, server_session)
}

fn bench_e2e_stream_open_and_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_stream_open_and_send");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("open_and_send", size), size, |b, &size| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let (client_session, _server_session) = create_connected_sessions().await;

                    // Open stream
                    let (stream, _synack_rx) = client_session.open_stream().await.unwrap();

                    // Send data
                    let data = Bytes::from(vec![0u8; size]);
                    let _ = stream.send_data(data);

                    black_box(&stream);
                })
        });
    }

    group.finish();
}

fn bench_e2e_multiple_streams_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_multiple_streams");

    for stream_count in [1usize, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_streams", stream_count),
            stream_count,
            |b, &count| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let (client_session, _server_session) = create_connected_sessions().await;

                        let mut streams = Vec::new();
                        for _ in 0..count {
                            if let Ok((stream, _)) = client_session.open_stream().await {
                                streams.push(stream);
                            }
                        }

                        // Send data on all streams concurrently
                        for stream in streams.iter() {
                            let data = Bytes::from(vec![0u8; 1024]);
                            let _ = stream.send_data(data);
                        }

                        black_box(streams);
                    })
            },
        );
    }

    group.finish();
}

fn available_tcp_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn udp_over_tcp_roundtrip(iterations: usize, payload_size: usize) -> Result<()> {
    let password = "bench_password";
    let port = available_tcp_port()?;
    let server_addr = format!("127.0.0.1:{port}");

    let server_config = tls::create_server_config()?;
    let tls_acceptor = Arc::new(TlsAcceptor::from(server_config));
    let server = Arc::new(Server::new(
        password,
        tls_acceptor,
        PaddingFactory::default(),
        None,
    ));

    let server_clone = server.clone();
    let server_addr_for_task = server_addr.clone();
    let server_task = tokio::spawn(async move {
        if let Err(e) = server_clone.listen(&server_addr_for_task).await {
            tracing::error!("[Bench] Server error: {}", e);
        }
    });

    sleep(Duration::from_millis(200)).await;

    let echo_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let echo_addr = echo_socket.local_addr()?;
    let echo_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match echo_socket.recv_from(&mut buf).await {
                Ok((len, peer)) => {
                    if len == 0 {
                        continue;
                    }
                    let _ = echo_socket.send_to(&buf[..len], peer).await;
                }
                Err(e) => {
                    tracing::error!("[Bench] Echo server error: {}", e);
                    break;
                }
            }
        }
    });

    let client_config = tls::create_client_config(None)?;
    let tls_connector = Arc::new(TlsConnector::from(client_config));
    let client = Arc::new(Client::new(
        password,
        server_addr.clone(),
        tls_connector,
        PaddingFactory::default(),
    ));

    let proxy_addr = client.create_udp_proxy("127.0.0.1:0", echo_addr).await?;

    sleep(Duration::from_millis(100)).await;

    let app_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let payload = vec![0u8; payload_size];
    let mut buf = vec![0u8; payload_size + 64];

    for _ in 0..iterations {
        app_socket.send_to(&payload, proxy_addr).await?;
        let (len, _addr) =
            timeout(Duration::from_secs(3), app_socket.recv_from(&mut buf)).await??;
        assert_eq!(len, payload_size);
    }

    drop(app_socket);
    drop(client);

    server_task.abort();
    let _ = server_task.await;
    echo_task.abort();
    let _ = echo_task.await;

    Ok(())
}

fn bench_udp_over_tcp_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("udp_over_tcp_roundtrip");
    for &(payload_size, iterations) in &[(64usize, 5usize), (256, 5), (1024, 3)] {
        group.bench_with_input(
            BenchmarkId::new("roundtrip", payload_size),
            &(payload_size, iterations),
            |b, &(size, iters)| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async move {
                        udp_over_tcp_roundtrip(iters, size)
                            .await
                            .expect("udp over tcp roundtrip benchmark failed");
                    })
            },
        );
    }
    group.finish();
}

fn bench_e2e_data_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_data_throughput");

    for (size, iterations) in [(64, 1000), (256, 500), (1024, 200), (4096, 50)].iter() {
        group.bench_with_input(
            BenchmarkId::new("throughput", size),
            &(size, iterations),
            |b, &(&size, &iters)| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let (client_session, _server_session) = create_connected_sessions().await;
                        let (stream, _synack_rx) = client_session.open_stream().await.unwrap();

                        let data = Bytes::from(vec![0u8; size]);
                        for _ in 0..iters {
                            let _ = stream.send_data(data.clone());
                        }

                        black_box(&stream);
                    })
            },
        );
    }

    group.finish();
}

fn bench_e2e_session_startup_and_streams(c: &mut Criterion) {
    c.bench_function("e2e_session_startup_and_streams", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let (client_session, server_session) = create_connected_sessions().await;

                // Start client (sends Settings frame)
                let client_clone = client_session.clone();
                tokio::spawn(async move {
                    let _ = client_clone.start_client().await;
                });

                // Start server background tasks
                let server_clone = server_session.clone();
                tokio::spawn(async move {
                    let _ = server_clone.recv_loop().await;
                });

                let server_clone2 = server_session.clone();
                tokio::spawn(async move {
                    let _ = server_clone2.process_stream_data().await;
                });

                // Open a stream
                let (stream, _synack_rx) = client_session.open_stream().await.unwrap();

                // Send some data
                let data = Bytes::from(vec![0u8; 1024]);
                let _ = stream.send_data(data);

                black_box((client_session, server_session, stream));
            })
    });
}

criterion_group!(
    benches,
    bench_e2e_stream_open_and_send,
    bench_e2e_multiple_streams_concurrent,
    bench_e2e_data_throughput,
    bench_e2e_session_startup_and_streams,
    bench_udp_over_tcp_roundtrip
);
criterion_main!(benches);
