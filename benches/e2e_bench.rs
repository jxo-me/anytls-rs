//! End-to-end performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench e2e_bench
//!
//! These benchmarks measure complete data flow through client and server sessions

use anytls_rs::padding::PaddingFactory;
use anytls_rs::session::Session;
use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

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

    for stream_count in [1, 5, 10, 20].iter() {
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
    bench_e2e_session_startup_and_streams
);
criterion_main!(benches);
