//! Common test utilities and helpers

use anytls_rs::{
    client::{Client, SessionPoolConfig},
    server::Server,
    util::tls,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
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

#[allow(dead_code)]
pub fn new_test_config() -> anyhow::Result<TestConfig> {
    let server_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        port
    };

    let client_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        port
    };

    Ok(TestConfig {
        server_addr: format!("127.0.0.1:{server_port}"),
        client_listen: format!("127.0.0.1:{client_port}"),
        ..Default::default()
    })
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
#[allow(dead_code)]
pub async fn create_test_client(config: &TestConfig) -> anyhow::Result<Arc<Client>> {
    create_test_client_with_config(config, SessionPoolConfig::default()).await
}

/// Create a test client with custom session pool configuration
pub async fn create_test_client_with_config(
    config: &TestConfig,
    pool_config: SessionPoolConfig,
) -> anyhow::Result<Arc<Client>> {
    let client_config = tls::create_client_config(None)?;
    let tls_connector = Arc::new(tokio_rustls::TlsConnector::from(client_config));
    let padding = anytls_rs::padding::PaddingFactory::default();

    let client = Arc::new(Client::with_pool_config(
        &config.password,
        config.server_addr.clone(),
        tls_connector,
        padding,
        pool_config,
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

/// Spawn a simple TCP echo server for tests, returning its address and join handle.
#[allow(dead_code)]
pub async fn spawn_tcp_echo_server() -> anyhow::Result<(std::net::SocketAddr, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, _peer)) => {
                    tokio::spawn(async move {
                        let mut buf = [0u8; 1024];
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if stream.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("[Test Echo] Accept error: {e}");
                    break;
                }
            }
        }
    });

    Ok((addr, handle))
}
