//! Edge cases and special scenario performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench edge_cases_bench
//!
//! These benchmarks test extreme cases: small packets, large packets, high-frequency operations

use anytls_rs::padding::PaddingFactory;
use anytls_rs::protocol::{Frame, Command};
use anytls_rs::protocol::FrameCodec;
use anytls_rs::session::Session;
use bytes::{Bytes, BytesMut};
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
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

fn bench_small_packets(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_packets");
    
    // Test very small packet sizes (1-64 bytes)
    for size in [1, 4, 8, 16, 32, 64].iter() {
        group.bench_with_input(
            BenchmarkId::new("encode", size),
            size,
            |b, &size| {
                let mut codec = FrameCodec;
                let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; size]));
                
                b.iter(|| {
                    let mut buffer = BytesMut::new();
                    let _ = codec.encode(frame.clone(), &mut buffer);
                    black_box(&buffer);
                })
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("write_frame", size),
            size,
            |b, &size| {
                b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                    let session = create_test_session().await;
                    let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; size]));
                    let _ = session.write_frame(frame).await;
                    black_box(&session);
                })
            },
        );
    }
    
    group.finish();
}

fn bench_large_packets(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_packets");
    
    // Test large packet sizes (1MB and above)
    for size in [1024 * 1024, 2 * 1024 * 1024, 4 * 1024 * 1024].iter() {
        group.bench_with_input(
            BenchmarkId::new("encode", format!("{}MB", size / (1024 * 1024))),
            size,
            |b, &size| {
                let mut codec = FrameCodec;
                let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; size]));
                
                b.iter(|| {
                    let mut buffer = BytesMut::new();
                    let _ = codec.encode(frame.clone(), &mut buffer);
                    black_box(&buffer);
                })
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("create_bytes", format!("{}MB", size / (1024 * 1024))),
            size,
            |b, &size| {
                b.iter(|| {
                    let bytes = Bytes::from(vec![0u8; size]);
                    black_box(bytes);
                })
            },
        );
    }
    
    group.finish();
}

fn bench_high_frequency_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("high_frequency_operations");
    
    // Test heartbeat frame encoding performance
    group.bench_function("heartbeat_frame_encode", |b| {
        let mut codec = FrameCodec;
        let frame = Frame::control(Command::HeartRequest, 0);
        
        b.iter(|| {
            let mut buffer = BytesMut::new();
            let _ = codec.encode(frame.clone(), &mut buffer);
            black_box(&buffer);
        })
    });
    
    // Test SYNACK frame encoding performance
    group.bench_function("synack_frame_encode", |b| {
        let mut codec = FrameCodec;
        let frame = Frame::control(Command::SynAck, 1);
        
        b.iter(|| {
            let mut buffer = BytesMut::new();
            let _ = codec.encode(frame.clone(), &mut buffer);
            black_box(&buffer);
        })
    });
    
    // Test high-frequency control frame writes
    group.bench_function("high_frequency_control_frames", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let session = create_test_session().await;
            
            // Write multiple control frames in sequence
            for i in 0..100 {
                let frame = Frame::control(Command::HeartRequest, i);
                let _ = session.write_control_frame(frame).await;
            }
            
            black_box(&session);
        })
    });
    
    group.finish();
}

fn bench_rapid_stream_creation(c: &mut Criterion) {
    c.bench_function("rapid_stream_creation", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let session = create_test_session().await;
            
            // Create many streams rapidly
            let mut streams = Vec::new();
            for _ in 0..100 {
                if let Ok((stream, _)) = session.open_stream().await {
                    streams.push(stream);
                }
            }
            
            black_box(streams);
        })
    });
}

criterion_group!(
    benches,
    bench_small_packets,
    bench_large_packets,
    bench_high_frequency_operations,
    bench_rapid_stream_creation
);
criterion_main!(benches);

