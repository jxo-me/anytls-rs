//! Performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench

use anytls_rs::padding::PaddingFactory;
use anytls_rs::protocol::{Command, Frame};
use anytls_rs::session::Session;
use anytls_rs::util::auth;
use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
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
            padding.clone(),
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
    use anytls_rs::protocol::FrameCodec;
    use bytes::BytesMut;
    use tokio_util::codec::Encoder;

    let mut group = c.benchmark_group("frame_encoding");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        let data = vec![0u8; *size];
        let frame = Frame::with_data(Command::Push, 1, Bytes::from(data));
        let mut codec = FrameCodec;

        group.bench_with_input(BenchmarkId::new("encode", size), &frame, |b, frame| {
            let mut buffer = BytesMut::new();
            b.iter(|| {
                buffer.clear();
                codec.encode(frame.clone(), &mut buffer).unwrap();
                black_box(&buffer);
            })
        });
    }

    group.finish();
}

fn bench_stream_creation(c: &mut Criterion) {
    c.bench_function("stream_creation", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let session = create_test_session_async(true).await;
                let session = Arc::new(session);
                let _stream = session.open_stream().await;
                black_box(_stream)
            })
    });
}

fn bench_session_startup(c: &mut Criterion) {
    c.bench_function("session_startup", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let session = create_test_session_async(true).await;
                // Note: start_client consumes the Arc, so we can't benchmark it directly
                // This benchmark just measures session creation
                black_box(session)
            })
    });
}

fn bench_session_startup_complete(c: &mut Criterion) {
    c.bench_function("session_startup_complete", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let session = create_test_session_async(true).await;
                let session = Arc::new(session);
                // Start client session (sends Settings frame and starts background tasks)
                let _ = session.clone().start_client().await;
                black_box(session)
            })
    });
}

fn bench_frame_decode(c: &mut Criterion) {
    use anytls_rs::protocol::FrameCodec;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    let mut group = c.benchmark_group("frame_decode");
    let mut codec = FrameCodec;

    for size in [64, 256, 1024, 4096, 16384].iter() {
        // Pre-encode frames for decoding benchmark
        let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; *size]));
        let mut encoded = BytesMut::new();
        codec.encode(frame, &mut encoded).unwrap();

        group.bench_with_input(BenchmarkId::new("decode", size), &encoded, |b, encoded| {
            let mut buffer = encoded.clone();
            b.iter(|| {
                let mut decode_codec = FrameCodec;
                let result = decode_codec.decode(&mut buffer);
                if let Ok(Some(frame)) = result {
                    black_box(frame);
                }
                // Reset buffer for next iteration
                buffer = encoded.clone();
            })
        });
    }

    group.finish();
}

fn bench_padding_factory(c: &mut Criterion) {
    // 测试 default() 调用开销（真实场景：通常在初始化时调用一次）
    c.bench_function("padding_factory_default", |b| {
        b.iter(|| {
            let _f = PaddingFactory::default();
            black_box(_f)
        })
    });

    // 测试实际使用场景：获取 factory 并使用（更真实）
    let factory = PaddingFactory::default();
    c.bench_function("padding_factory_get_and_use", |b| {
        b.iter(|| {
            // 模拟实际使用：获取并调用方法
            let f = PaddingFactory::default();
            let sizes = f.generate_record_payload_sizes(0);
            black_box(sizes);
        })
    });

    // 测试 generate_sizes 性能（重用 factory，避免 default() 开销）
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
    let passwords = [
        "short",
        "medium_length_password",
        "very_long_password_that_exceeds_normal_length",
    ];

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

fn bench_session_write_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_write_frame");

    for size in [64, 256, 1024, 4096].iter() {
        group.bench_with_input(BenchmarkId::new("write_frame", size), size, |b, &size| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let session = create_test_session_async(true).await;
                    let session = Arc::new(session);
                    let frame = Frame::with_data(Command::Push, 1, Bytes::from(vec![0u8; size]));
                    let _ = session.write_frame(frame).await;
                    black_box(&session);
                })
        });
    }

    group.finish();
}

fn bench_session_write_data_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_write_data_frame");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(
            BenchmarkId::new("write_data_frame", size),
            size,
            |b, &size| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let session = create_test_session_async(true).await;
                        let session = Arc::new(session);
                        let data = Bytes::from(vec![0u8; size]);
                        let _ = session.write_data_frame(1, data).await;
                        black_box(&session);
                    })
            },
        );
    }

    group.finish();
}

fn bench_session_control_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_control_frames");

    // Test different control frame types
    let frame_types = [
        (Command::Syn, "syn"),
        (Command::Fin, "fin"),
        (Command::HeartRequest, "heart_request"),
        (Command::HeartResponse, "heart_response"),
    ];

    for (cmd, name) in frame_types.iter() {
        group.bench_with_input(BenchmarkId::from_parameter(name), cmd, |b, &cmd| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let session = create_test_session_async(true).await;
                    let session = Arc::new(session);
                    let frame = Frame::control(cmd, 1);
                    let _ = session.write_control_frame(frame).await;
                    black_box(&session);
                })
        });
    }

    group.finish();
}

fn bench_session_multiple_streams(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_multiple_streams");

    for stream_count in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("open_streams", stream_count),
            stream_count,
            |b, &count| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let session = create_test_session_async(true).await;
                        let session = Arc::new(session);

                        let mut streams = Vec::new();
                        for _ in 0..count {
                            if let Ok((stream, _)) = session.open_stream().await {
                                streams.push(stream);
                            }
                        }
                        black_box(streams);
                    })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_frame_encoding,
    bench_frame_decode,
    bench_stream_creation,
    bench_session_startup,
    bench_session_startup_complete,
    bench_session_write_frame,
    bench_session_write_data_frame,
    bench_session_control_frames,
    bench_session_multiple_streams,
    bench_padding_factory,
    bench_password_hashing
);
criterion_main!(benches);
