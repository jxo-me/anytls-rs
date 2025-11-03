//! Stream performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench stream_bench

use anytls_rs::padding::PaddingFactory;
use anytls_rs::session::{Session, StreamReader};
use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

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
    ))
}

async fn create_test_stream() -> (Arc<anytls_rs::session::Stream>, Arc<Session>) {
    let session = create_test_session().await;
    let (stream, _synack_rx) = session.open_stream().await.unwrap();
    (stream, session)
}

fn bench_stream_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_write");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("write", size), size, |b, &size| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let (stream, _session) = create_test_stream().await;
                    let data = Bytes::from(vec![0u8; size]);
                    let _ = stream.send_data(data);
                    black_box(&stream);
                })
        });
    }

    group.finish();
}

fn bench_stream_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_read");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("read", size), size, |b, &size| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    // Create StreamReader directly with pre-populated data
                    // This tests the read mechanism without needing Session integration
                    let (receive_tx, receive_rx) = mpsc::unbounded_channel();
                    let mut reader = StreamReader::new(1, receive_rx);

                    // Pre-populate with data
                    let data = Bytes::from(vec![0u8; size]);
                    let _ = receive_tx.send(data);

                    // Now test reading - this should return data immediately
                    let mut buffer = vec![0u8; size];
                    let n = reader.read(&mut buffer).await.unwrap_or(0);
                    black_box(&buffer[..n]);
                })
        });
    }

    group.finish();
}

fn bench_streamreader_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("streamreader_read");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("read", size), size, |b, &size| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let (receive_tx, receive_rx) = mpsc::unbounded_channel();
                    let mut reader = StreamReader::new(1, receive_rx);

                    // Pre-populate with data
                    let data = Bytes::from(vec![0u8; size]);
                    let _ = receive_tx.send(data);

                    let mut buffer = vec![0u8; size];
                    let _ = reader.read(&mut buffer).await;
                    black_box(&buffer);
                })
        });
    }

    group.finish();
}

fn bench_stream_concurrent_read_write(c: &mut Criterion) {
    c.bench_function("stream_concurrent_read_write", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let (stream, _session) = create_test_stream().await;

                // Create a separate StreamReader for read testing
                let (read_tx, read_rx) = mpsc::unbounded_channel();
                let mut read_reader = StreamReader::new(stream.id(), read_rx);

                // Spawn concurrent write task
                let write_stream = stream.clone();
                let write_handle = tokio::spawn(async move {
                    for _ in 0..100 {
                        let data = Bytes::from(vec![0u8; 1024]);
                        let _ = write_stream.send_data(data);
                    }
                });

                // Spawn concurrent read task
                let read_handle = tokio::spawn(async move {
                    // Pre-populate with data for read testing
                    for _ in 0..100 {
                        let data = Bytes::from(vec![0u8; 1024]);
                        let _ = read_tx.send(data);
                    }

                    let mut buffer = vec![0u8; 1024];
                    // Read data - should not block since we pre-populated
                    for _ in 0..100 {
                        let _ = read_reader.read(&mut buffer).await;
                    }
                });

                let _ = tokio::try_join!(write_handle, read_handle);
                black_box(&stream);
            })
    });
}

criterion_group!(
    benches,
    bench_stream_write,
    bench_stream_read,
    bench_streamreader_read,
    bench_stream_concurrent_read_write
);
criterion_main!(benches);
