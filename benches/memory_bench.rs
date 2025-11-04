//! Memory allocation performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench memory_bench
//!
//! These benchmarks measure memory allocation and copy performance

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench_bytes_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_allocation");

    for size in [64, 256, 1024, 4096, 16384, 65536].iter() {
        group.bench_with_input(BenchmarkId::new("from_vec", size), size, |b, &size| {
            b.iter(|| {
                let data = vec![0u8; size];
                let bytes = Bytes::from(data);
                black_box(bytes);
            })
        });

        group.bench_with_input(
            BenchmarkId::new("copy_from_slice", size),
            size,
            |b, &size| {
                let source = vec![0u8; size];
                b.iter(|| {
                    let bytes = Bytes::copy_from_slice(&source);
                    black_box(bytes);
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("from_static", size), size, |b, &size| {
            // Use a larger static buffer for comparison
            const STATIC_BUF: &[u8] = &[0u8; 65536];
            b.iter(|| {
                let bytes = Bytes::from_static(&STATIC_BUF[..size]);
                black_box(bytes);
            })
        });
    }

    group.finish();
}

fn bench_bytes_clone_vs_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_clone_vs_copy");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        let original = Bytes::from(vec![0u8; *size]);

        group.bench_with_input(BenchmarkId::new("clone", size), &original, |b, bytes| {
            b.iter(|| {
                let cloned = bytes.clone();
                black_box(cloned);
            })
        });

        group.bench_with_input(
            BenchmarkId::new("copy_from_slice", size),
            &original,
            |b, bytes| {
                b.iter(|| {
                    let copied = Bytes::copy_from_slice(bytes);
                    black_box(copied);
                })
            },
        );
    }

    group.finish();
}

fn bench_bytes_slice(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_slice");

    for size in [1024, 4096, 16384].iter() {
        let bytes = Bytes::from(vec![0u8; *size]);

        group.bench_with_input(
            BenchmarkId::new("slice_middle", size),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    let start = bytes.len() / 4;
                    let end = bytes.len() * 3 / 4;
                    let sliced = bytes.slice(start..end);
                    black_box(sliced);
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("slice_ref", size), &bytes, |b, bytes| {
            b.iter(|| {
                let start = bytes.len() / 4;
                let end = bytes.len() * 3 / 4;
                let slice_ref = &bytes[start..end];
                black_box(slice_ref);
            })
        });
    }

    group.finish();
}

fn bench_vec_vs_bytes_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec_vs_bytes_allocation");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.bench_with_input(BenchmarkId::new("vec_new", size), size, |b, &size| {
            b.iter(|| {
                let vec = Vec::<u8>::with_capacity(size);
                black_box(vec);
            })
        });

        group.bench_with_input(BenchmarkId::new("vec_from", size), size, |b, &size| {
            b.iter(|| {
                let vec = vec![0u8; size];
                black_box(vec);
            })
        });

        group.bench_with_input(
            BenchmarkId::new("bytes_from_vec", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let vec = vec![0u8; size];
                    let bytes = Bytes::from(vec);
                    black_box(bytes);
                })
            },
        );
    }

    group.finish();
}

fn bench_memory_reuse_patterns(c: &mut Criterion) {
    c.bench_function("memory_reuse_buffer", |b| {
        let mut buffer = Vec::with_capacity(16384);
        b.iter(|| {
            buffer.clear();
            buffer.extend_from_slice(&vec![0u8; 1024]);
            black_box(&buffer);
        })
    });

    c.bench_function("memory_allocate_each_time", |b| {
        b.iter(|| {
            let buffer = vec![0u8; 1024];
            black_box(buffer);
        })
    });
}

criterion_group!(
    benches,
    bench_bytes_allocation,
    bench_bytes_clone_vs_copy,
    bench_bytes_slice,
    bench_vec_vs_bytes_allocation,
    bench_memory_reuse_patterns
);
criterion_main!(benches);
