//! Common test utilities and helpers

use anytls_rs::{client::Client, server::Server, util::tls};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

/// Test configuration
#[allow(dead_code)]
pub struct TestConfig {
    pub server_addr: String,
    pub client_listen: String,
    pub password: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:8443".to_string(),
            client_listen: "127.0.0.1:1080".to_string(),
            password: "test_password".to_string(),
        }
    }
}

/// Create a test server instance
pub async fn create_test_server(config: &TestConfig) -> anyhow::Result<Arc<Server>> {
    let server_config = tls::create_server_config()?;
    let tls_acceptor = Arc::new(tokio_rustls::TlsAcceptor::from(server_config));
    let padding = anytls_rs::padding::PaddingFactory::default();

    let server = Arc::new(Server::new(&config.password, tls_acceptor, padding, None));

    Ok(server)
}

/// Create a test client instance
pub async fn create_test_client(config: &TestConfig) -> anyhow::Result<Arc<Client>> {
    let client_config = tls::create_client_config(None)?;
    let tls_connector = Arc::new(tokio_rustls::TlsConnector::from(client_config));
    let padding = anytls_rs::padding::PaddingFactory::default();

    let client = Arc::new(Client::new(
        &config.password,
        config.server_addr.clone(),
        tls_connector,
        padding,
    ));

    Ok(client)
}

/// Wait for a condition to become true (with timeout)
#[allow(dead_code)]
pub async fn wait_for<F>(mut condition: F, timeout: Duration) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Check if a port is listening
#[allow(dead_code)]
pub async fn is_port_listening(addr: &str) -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect(addr).await.is_ok()
}
