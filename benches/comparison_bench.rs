//! Performance comparison benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench comparison_bench
//!
//! These benchmarks help compare performance across different implementations
//! and track performance changes over time

use anytls_rs::padding::PaddingFactory;
use anytls_rs::protocol::FrameCodec;
use anytls_rs::protocol::{Command, Frame};
use anytls_rs::session::Session;
use bytes::{Bytes, BytesMut};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::Encoder;

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

/// Baseline performance metrics for comparison
/// These can be used to track performance regressions
fn bench_baseline_frame_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_frame_encoding");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("baseline", size), size, |b, &size| {
            let mut codec = FrameCodec;
            let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; size]));

            b.iter(|| {
                let mut buffer = BytesMut::new();
                let _ = codec.encode(frame.clone(), &mut buffer);
                black_box(&buffer);
            })
        });
    }

    group.finish();
}

/// Compare different frame encoding strategies
fn bench_frame_encoding_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_encoding_strategies");

    for size in [1024, 4096].iter() {
        let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; *size]));

        // Strategy 1: Encode each time
        group.bench_with_input(
            BenchmarkId::new("encode_each_time", size),
            &frame,
            |b, frame| {
                let mut codec = FrameCodec;
                b.iter(|| {
                    let mut buffer = BytesMut::new();
                    let _ = codec.encode(frame.clone(), &mut buffer);
                    black_box(&buffer);
                })
            },
        );

        // Strategy 2: Pre-encode and clone
        let mut codec = FrameCodec;
        let mut pre_encoded = BytesMut::new();
        codec.encode(frame.clone(), &mut pre_encoded).unwrap();
        let encoded_bytes = Bytes::from(pre_encoded);

        group.bench_with_input(
            BenchmarkId::new("pre_encoded_clone", size),
            &encoded_bytes,
            |b, encoded| {
                b.iter(|| {
                    let cloned = encoded.clone();
                    black_box(cloned);
                })
            },
        );
    }

    group.finish();
}

/// Performance tracking: Stream creation overhead
fn bench_stream_creation_overhead(c: &mut Criterion) {
    c.bench_function("stream_creation_overhead", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let mock_stream = MockStream::new();
                let (reader, writer) = tokio::io::split(mock_stream);
                let padding = PaddingFactory::default();

                let session = Arc::new(Session::new_client(
                    Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
                    Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
                    padding,
                ));

                let (stream, _synack_rx) = session.open_stream().await.unwrap();
                black_box(stream);
            })
    });
}

/// Performance tracking: Session startup overhead
fn bench_session_startup_overhead(c: &mut Criterion) {
    c.bench_function("session_startup_overhead", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let mock_stream = MockStream::new();
                let (reader, writer) = tokio::io::split(mock_stream);
                let padding = PaddingFactory::default();

                let session = Arc::new(Session::new_client(
                    Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
                    Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
                    padding,
                ));

                let _ = session.clone().start_client().await;
                black_box(&session);
            })
    });
}

/// Performance tracking: Data frame write throughput
fn bench_data_frame_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_frame_throughput");

    for (size, count) in [(1024, 1000), (4096, 500), (16384, 200)].iter() {
        group.bench_with_input(
            BenchmarkId::new("throughput", format!("{}B_x{}", size, count)),
            &(size, count),
            |b, &(&size, &count)| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let mock_stream = MockStream::new();
                        let (reader, writer) = tokio::io::split(mock_stream);
                        let padding = PaddingFactory::default();

                        let session = Arc::new(Session::new_client(
                            Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
                            Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
                            padding,
                        ));

                        let data = Bytes::from(vec![0u8; size]);
                        for _ in 0..count {
                            let _ = session.write_data_frame(1, data.clone()).await;
                        }

                        black_box(&session);
                    })
            },
        );
    }

    group.finish();
}

/// Performance regression detection: Critical path operations
fn bench_critical_path_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("critical_path_operations");

    // Critical operation 1: Frame encoding (most frequent)
    group.bench_function("frame_encode_critical", |b| {
        let mut codec = FrameCodec;
        let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; 1024]));

        b.iter(|| {
            let mut buffer = BytesMut::new();
            let _ = codec.encode(frame.clone(), &mut buffer);
            black_box(&buffer);
        })
    });

    // Critical operation 2: Bytes creation (very frequent)
    group.bench_function("bytes_creation_critical", |b| {
        let source = vec![0u8; 1024];
        b.iter(|| {
            let bytes = Bytes::copy_from_slice(&source);
            black_box(bytes);
        })
    });

    // Critical operation 3: Stream data send (high frequency)
    group.bench_function("stream_send_critical", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let mock_stream = MockStream::new();
                let (reader, writer) = tokio::io::split(mock_stream);
                let padding = PaddingFactory::default();

                let session = Arc::new(Session::new_client(
                    Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
                    Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
                    padding,
                ));

                let (stream, _synack_rx) = session.open_stream().await.unwrap();
                let data = Bytes::from(vec![0u8; 1024]);
                let _ = stream.send_data(data);

                black_box(&stream);
            })
    });

    group.finish();
}

// Note: Go version comparison would require:
// 1. Go benchmark implementation
// 2. External tool to run Go benchmarks
// 3. Comparison script to analyze results
// This is marked as future work requiring integration

criterion_group!(
    benches,
    bench_baseline_frame_encoding,
    bench_frame_encoding_strategies,
    bench_stream_creation_overhead,
    bench_session_startup_overhead,
    bench_data_frame_throughput,
    bench_critical_path_operations
);
criterion_main!(benches);
