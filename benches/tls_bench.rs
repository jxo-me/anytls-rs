//! TLS performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench tls_bench
//!
//! These benchmarks measure TLS configuration and certificate generation performance

use anytls_rs::util::tls;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn bench_tls_generate_key_pair(c: &mut Criterion) {
    c.bench_function("tls_generate_key_pair", |b| {
        b.iter(|| {
            let result = tls::generate_key_pair();
            let _ = black_box(result);
        })
    });
}

fn bench_tls_generate_key_pair_with_name(c: &mut Criterion) {
    c.bench_function("tls_generate_key_pair_with_name", |b| {
        b.iter(|| {
            let result = tls::generate_key_pair_with_name(Some("example.com"));
            let _ = black_box(result);
        })
    });
}

fn bench_tls_create_server_config(c: &mut Criterion) {
    c.bench_function("tls_create_server_config", |b| {
        b.iter(|| {
            let result = tls::create_server_config();
            let _ = black_box(result);
        })
    });
}

fn bench_tls_create_client_config(c: &mut Criterion) {
    c.bench_function("tls_create_client_config", |b| {
        b.iter(|| {
            let result = tls::create_client_config(None);
            let _ = black_box(result);
        })
    });
}

fn bench_tls_config_reuse(c: &mut Criterion) {
    c.bench_function("tls_config_reuse", |b| {
        // Create config once and reuse
        let server_config = tls::create_server_config().unwrap();
        let client_config = tls::create_client_config(None).unwrap();
        
        b.iter(|| {
            black_box(&server_config);
            black_box(&client_config);
        })
    });
}

criterion_group!(
    benches,
    bench_tls_generate_key_pair,
    bench_tls_generate_key_pair_with_name,
    bench_tls_create_server_config,
    bench_tls_create_client_config,
    bench_tls_config_reuse
);
criterion_main!(benches);

