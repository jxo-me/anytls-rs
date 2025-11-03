//! Client/Server performance benchmarks for AnyTLS-RS
//!
//! Run with: cargo bench --bench client_server_bench
//!
//! These benchmarks measure end-to-end Client/Server performance

use anytls_rs::client::Client;
use anytls_rs::padding::PaddingFactory;
use anytls_rs::util::tls;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Arc;
use tokio_rustls::TlsConnector;

fn bench_client_creation(c: &mut Criterion) {
    c.bench_function("client_creation", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let tls_config = tls::create_client_config(None).unwrap();
                let tls_connector = TlsConnector::from(tls_config);
                let padding = PaddingFactory::default();

                let client = Client::new(
                    "test_password",
                    "localhost:8443".to_string(),
                    Arc::new(tls_connector),
                    padding,
                );
                black_box(client);
            })
    });
}

fn bench_client_with_pool_config(c: &mut Criterion) {
    use anytls_rs::client::SessionPoolConfig;
    use tokio::time::Duration;

    c.bench_function("client_with_pool_config", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let tls_config = tls::create_client_config(None).unwrap();
                let tls_connector = TlsConnector::from(tls_config);
                let padding = PaddingFactory::default();
                let pool_config = SessionPoolConfig {
                    check_interval: Duration::from_secs(30),
                    idle_timeout: Duration::from_secs(60),
                    min_idle_sessions: 2,
                };

                let client = Client::with_pool_config(
                    "test_password",
                    "localhost:8443".to_string(),
                    Arc::new(tls_connector),
                    padding,
                    pool_config,
                );
                black_box(client);
            })
    });
}

fn bench_tls_connector_creation(c: &mut Criterion) {
    c.bench_function("tls_connector_creation", |b| {
        b.iter(|| {
            let tls_config = tls::create_client_config(None).unwrap();
            let tls_connector = TlsConnector::from(tls_config);
            black_box(Arc::new(tls_connector));
        })
    });
}

fn bench_tls_connector_reuse(c: &mut Criterion) {
    c.bench_function("tls_connector_reuse", |b| {
        let tls_config = tls::create_client_config(None).unwrap();
        let tls_connector = Arc::new(TlsConnector::from(tls_config));

        b.iter(|| {
            black_box(&tls_connector);
        })
    });
}

fn bench_client_password_hashing(c: &mut Criterion) {
    use anytls_rs::util::hash_password;

    let passwords = ["short", "medium_password", "very_long_password_for_testing"];

    let mut group = c.benchmark_group("client_password_hashing");

    for password in passwords.iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(password.len()),
            password,
            |b, pwd| {
                b.iter(|| {
                    let hash = hash_password(pwd);
                    black_box(hash);
                })
            },
        );
    }

    group.finish();
}

// Note: Full client-server benchmarks would require actual network connections
// These are marked as future work or integration tests
fn bench_client_server_setup_components(c: &mut Criterion) {
    c.bench_function("client_server_setup_components", |b| {
        b.iter(|| {
            // Create all necessary components for client/server setup
            let server_tls_config = tls::create_server_config().unwrap();
            let client_tls_config = tls::create_client_config(None).unwrap();
            let padding = PaddingFactory::default();

            black_box((server_tls_config, client_tls_config, padding));
        })
    });
}

criterion_group!(
    benches,
    bench_client_creation,
    bench_client_with_pool_config,
    bench_tls_connector_creation,
    bench_tls_connector_reuse,
    bench_client_password_hashing,
    bench_client_server_setup_components
);
criterion_main!(benches);
