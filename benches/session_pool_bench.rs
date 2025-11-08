//! Session Pool performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench session_pool_bench
//!
//! These benchmarks measure Session Pool performance for connection reuse

use anytls_rs::client::session_pool::{SessionPool, SessionPoolConfig};
use anytls_rs::padding::PaddingFactory;
use anytls_rs::session::Session;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::Duration;

/// Mock async reader/writer for testing
struct MockStream;

impl MockStream {
    fn new() -> Self {
        Self
    }
}

impl AsyncRead for MockStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl Unpin for MockStream {}

async fn create_test_session() -> Arc<Session> {
    let mock_stream = MockStream::new();
    let (reader, writer) = tokio::io::split(mock_stream);
    let padding = PaddingFactory::default();

    Arc::new(Session::new_client(
        Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
        Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
        padding,
        None,
    ))
}

fn bench_session_pool_add_and_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_pool_add_and_get");

    for session_count in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("add_and_get", session_count),
            session_count,
            |b, &count| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let pool = SessionPool::new();

                        // Add sessions to pool
                        for _ in 0..count {
                            let session = create_test_session().await;
                            pool.add_idle_session(session).await;
                        }

                        // Get all sessions back
                        let mut sessions = Vec::new();
                        for _ in 0..count {
                            if let Some(session) = pool.get_idle_session().await {
                                sessions.push(session);
                            }
                        }

                        black_box(sessions);
                    })
            },
        );
    }

    group.finish();
}

fn bench_session_pool_concurrent_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_pool_concurrent_get");

    for (pool_size, concurrent_gets) in [(10, 5), (20, 10), (50, 20)].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                "concurrent_get",
                format!("{}pool_{}gets", pool_size, concurrent_gets),
            ),
            &(pool_size, concurrent_gets),
            |b, &(&pool_size, &concurrent_gets)| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let pool = Arc::new(SessionPool::new());

                        // Pre-populate pool
                        for _ in 0..pool_size {
                            let session = create_test_session().await;
                            pool.add_idle_session(session).await;
                        }

                        // Concurrently get sessions
                        let handles: Vec<_> = (0..concurrent_gets)
                            .map(|_| {
                                let pool_clone = pool.clone();
                                tokio::spawn(async move { pool_clone.get_idle_session().await })
                            })
                            .collect();

                        let mut results = Vec::new();
                        for handle in handles {
                            if let Ok(result) = handle.await {
                                results.push(result);
                            }
                        }

                        black_box(results);
                    })
            },
        );
    }

    group.finish();
}

fn bench_session_pool_cleanup(c: &mut Criterion) {
    c.bench_function("session_pool_cleanup", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let config = SessionPoolConfig {
                    check_interval: Duration::from_secs(1),
                    idle_timeout: Duration::from_millis(100),
                    min_idle_sessions: 1,
                };
                let pool = SessionPool::with_config(config);

                // Add multiple sessions
                for _ in 0..10 {
                    let session = create_test_session().await;
                    pool.add_idle_session(session).await;
                }

                // Wait a bit for cleanup
                tokio::time::sleep(Duration::from_millis(150)).await;

                // Trigger cleanup
                pool.cleanup_expired().await;

                let idle_count = pool.idle_count().await;
                black_box(idle_count);
            })
    });
}

criterion_group!(
    benches,
    bench_session_pool_add_and_get,
    bench_session_pool_concurrent_get,
    bench_session_pool_cleanup
);
criterion_main!(benches);
