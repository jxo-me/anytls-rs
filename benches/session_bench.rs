//! Performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench

use anytls_rs::padding::PaddingFactory;
use anytls_rs::protocol::{Frame, Command};
use anytls_rs::session::Session;
use anytls_rs::util::{auth, tls};
use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::hint::black_box;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

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
        // Return EOF immediately (no data)
        std::task::Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        // Write data (ignore for benchmark, just return written length)
        let len = buf.len();
        std::task::Poll::Ready(Ok(len))
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

async fn create_test_session_async(is_client: bool) -> Session {
    // Create mock streams
    let mock_stream = MockStream::new();
    
    let (reader, writer) = tokio::io::split(mock_stream);
    let padding = PaddingFactory::default();
    
    if is_client {
        Session::new_client(
            Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
            Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
            padding,
        )
    } else {
        Session::new_server(
            Box::new(reader) as Box<dyn AsyncRead + Send + Unpin>,
            Box::new(writer) as Box<dyn AsyncWrite + Send + Unpin>,
            padding,
        )
    }
}

fn bench_frame_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_encoding");
    
    for size in [64, 256, 1024, 4096, 16384].iter() {
        let data = vec![0u8; *size];
        let frame = Frame::with_data(Command::Push, 1, Bytes::from(data));
        
        group.bench_with_input(
            BenchmarkId::new("encode", size),
            &frame,
            |b, frame| {
                b.iter(|| {
                    // Simulate encoding by accessing frame fields
                    black_box(frame.cmd);
                    black_box(frame.stream_id);
                    black_box(frame.data.len());
                })
            },
        );
    }
    
    group.finish();
}

fn bench_stream_creation(c: &mut Criterion) {
    c.bench_function("stream_creation", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let session = create_test_session_async(true).await;
            let session = Arc::new(session);
            let _stream = session.open_stream().await;
            black_box(_stream)
        })
    });
}

fn bench_session_startup(c: &mut Criterion) {
    c.bench_function("session_startup", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let session = create_test_session_async(true).await;
            // Note: start_client consumes the Arc, so we can't benchmark it directly
            // This benchmark just measures session creation
            black_box(session)
        })
    });
}

fn bench_padding_factory(c: &mut Criterion) {
    let factory = PaddingFactory::default();
    
    c.bench_function("padding_factory_default", |b| {
        b.iter(|| {
            let _f = PaddingFactory::default();
            black_box(_f)
        })
    });
    
    c.bench_function("padding_factory_generate_sizes", |b| {
        b.iter(|| {
            for i in 0..10 {
                let sizes = factory.generate_record_payload_sizes(i);
                black_box(sizes);
            }
        })
    });
}

fn bench_password_hashing(c: &mut Criterion) {
    let passwords = vec!["short", "medium_length_password", "very_long_password_that_exceeds_normal_length"];
    
    let mut group = c.benchmark_group("password_hashing");
    
    for password in passwords.iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(password.len()),
            password,
            |b, pwd| {
                b.iter(|| {
                    let hash = auth::hash_password(pwd);
                    black_box(hash)
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_frame_encoding,
    bench_stream_creation,
    bench_session_startup,
    bench_padding_factory,
    bench_password_hashing
);
criterion_main!(benches);

