//! Server connection handlers

use crate::protocol::{Command, Frame};
use crate::session::{Session, Stream};
use crate::util::{AnyTlsError, Result, configure_tcp_stream, resolve_host_addresses_with_cache};
use bytes::Bytes;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio::time::{Duration, Instant, sleep_until, timeout};

/// Handler trait for processing new streams
pub trait StreamHandler: Send + Sync {
    /// Handle a new stream
    fn handle_stream(
        &self,
        stream: Arc<Stream>,
        session: Arc<crate::session::Session>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;
}

/// Default stream handler that proxies TCP connections
pub struct TcpProxyHandler {
    // Destination will be read from stream
}

impl Default for TcpProxyHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpProxyHandler {
    /// Create a new TCP proxy handler
    pub fn new() -> Self {
        Self {}
    }
}

impl StreamHandler for TcpProxyHandler {
    fn handle_stream(
        &self,
        stream: Arc<Stream>,
        session: Arc<crate::session::Session>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let stream_id = stream.id();
            let peer_version = session.peer_version();
            tracing::debug!(
                "[Proxy] Handling stream {} (peer_version={})",
                stream_id,
                peer_version
            );

            // Read SOCKS5 address to determine the destination
            let destination = match read_socks_addr(stream.clone()).await {
                Ok(addr) => addr,
                Err(e) => {
                    tracing::error!("[Proxy] Failed to read SOCKS5 address: {}", e);
                    return Err(e);
                }
            };

            tracing::info!(
                "[Proxy] Destination: {}:{}",
                destination.addr,
                destination.port
            );

            // Check if this is a UDP over TCP request
            if let Some(legacy_v1) = uot_version(&destination.addr) {
                tracing::debug!("[Proxy] Detected UDP over TCP request");
                if peer_version >= 2 {
                    tracing::debug!(
                        "[Proxy] Sending SYNACK for UDP stream {} (connection established)",
                        stream_id
                    );
                    let synack_frame = Frame::control(Command::SynAck, stream_id);
                    if let Err(e) = session.write_control_frame(synack_frame).await {
                        tracing::error!("[Proxy] Failed to send SYNACK: {}", e);
                        return Err(e);
                    }
                }
                crate::server::udp_proxy::handle_udp_over_tcp(stream, legacy_v1).await
            } else {
                // Regular TCP proxy
                proxy_tcp_connection_with_synack_internal(
                    stream,
                    session,
                    stream_id,
                    peer_version,
                    destination,
                )
                .await
            }
        })
    }
}

fn uot_version(domain: &str) -> Option<bool> {
    if domain.eq_ignore_ascii_case(crate::client::udp_client::UDP_OVER_TCP_V1_MAGIC_ADDR) {
        Some(true)
    } else if domain.eq_ignore_ascii_case(crate::client::udp_client::UDP_OVER_TCP_MAGIC_ADDR) {
        Some(false)
    } else {
        None
    }
}

/// SOCKS5 address (similar to Go's M.Socksaddr)
#[derive(Debug, Clone)]
struct SocksAddr {
    addr: String,
    port: u16,
}

/// Read SOCKS5 address format from stream
/// Format: [ATYP (1 byte) | ADDR (variable) | PORT (2 bytes)]
///
/// 新实现：直接使用 StreamReader，无需额外的 Mutex 包装
async fn read_socks_addr(stream: Arc<Stream>) -> Result<SocksAddr> {
    let stream_id = stream.id();
    tracing::debug!(
        "[Proxy] read_socks_addr: Starting to read SOCKS5 address from stream {}",
        stream_id
    );

    // 获取 reader 的引用
    let reader_mutex = stream.reader();

    // Read ATYP byte first
    tracing::trace!(
        "[Proxy] read_socks_addr: Reading ATYP byte from stream {}",
        stream_id
    );
    let mut atyp_buf = [0u8; 1];
    {
        let mut reader = reader_mutex.lock().await;
        reader
            .read(&mut atyp_buf)
            .await
            .map_err(|e| AnyTlsError::Protocol(format!("Failed to read address type: {}", e)))?;
    }
    tracing::trace!(
        "[Proxy] read_socks_addr: Read ATYP={:02x} from stream {}",
        atyp_buf[0],
        stream_id
    );

    let atyp = atyp_buf[0];
    let addr =
        match atyp {
            0x01 => {
                // IPv4: 4 bytes
                tracing::trace!(
                    "[Proxy] read_socks_addr: Reading IPv4 address (stream {})",
                    stream_id
                );
                let mut ip_buf = [0u8; 4];
                {
                    let mut reader = reader_mutex.lock().await;
                    reader.read_exact(&mut ip_buf).await.map_err(|e| {
                        AnyTlsError::Protocol(format!("Failed to read IPv4: {}", e))
                    })?;
                }
                IpAddr::V4(Ipv4Addr::from(ip_buf)).to_string()
            }
            0x03 => {
                // Domain name: [LEN (1 byte) | DOMAIN (LEN bytes)]
                tracing::trace!(
                    "[Proxy] read_socks_addr: Reading domain name (stream {})",
                    stream_id
                );
                let mut len_buf = [0u8; 1];
                {
                    let mut reader = reader_mutex.lock().await;
                    reader.read_exact(&mut len_buf).await.map_err(|e| {
                        AnyTlsError::Protocol(format!("Failed to read domain length: {}", e))
                    })?;
                }

                let domain_len = len_buf[0] as usize;
                tracing::trace!(
                    "[Proxy] read_socks_addr: Domain length={} (stream {})",
                    domain_len,
                    stream_id
                );
                if domain_len == 0 || domain_len > 255 {
                    return Err(AnyTlsError::Protocol("Invalid domain length".to_string()));
                }

                let mut domain_buf = vec![0u8; domain_len];
                {
                    let mut reader = reader_mutex.lock().await;
                    reader.read_exact(&mut domain_buf).await.map_err(|e| {
                        AnyTlsError::Protocol(format!("Failed to read domain: {}", e))
                    })?;
                }

                String::from_utf8(domain_buf)
                    .map_err(|e| AnyTlsError::Protocol(format!("Invalid domain name: {}", e)))?
            }
            0x04 => {
                // IPv6: 16 bytes
                tracing::trace!(
                    "[Proxy] read_socks_addr: Reading IPv6 address (stream {})",
                    stream_id
                );
                let mut ip_buf = [0u8; 16];
                {
                    let mut reader = reader_mutex.lock().await;
                    reader.read_exact(&mut ip_buf).await.map_err(|e| {
                        AnyTlsError::Protocol(format!("Failed to read IPv6: {}", e))
                    })?;
                }
                IpAddr::V6(Ipv6Addr::from(ip_buf)).to_string()
            }
            _ => {
                return Err(AnyTlsError::Protocol(format!(
                    "Unsupported address type: 0x{:02x}",
                    atyp
                )));
            }
        };

    // Read port (2 bytes, big-endian)
    tracing::trace!(
        "[Proxy] read_socks_addr: Reading port (stream {})",
        stream_id
    );
    let mut port_buf = [0u8; 2];
    {
        let mut reader = reader_mutex.lock().await;
        reader
            .read_exact(&mut port_buf)
            .await
            .map_err(|e| AnyTlsError::Protocol(format!("Failed to read port: {}", e)))?;
    }
    let port = u16::from_be_bytes(port_buf);

    tracing::debug!(
        "[Proxy] read_socks_addr: Successfully read address {}:{} from stream {}",
        addr,
        port,
        stream_id
    );
    Ok(SocksAddr { addr, port })
}

/// Proxy TCP connection with SYNACK support (internal version with destination already provided)
async fn proxy_tcp_connection_with_synack_internal(
    stream: Arc<Stream>,
    session: Arc<Session>,
    stream_id: u32,
    peer_version: u8,
    destination: SocksAddr,
) -> Result<()> {
    tracing::debug!(
        "[Proxy] proxy_tcp_connection_with_synack: Starting for stream {} (peer_version={})",
        stream_id,
        peer_version
    );
    tracing::debug!(
        "[Proxy] Connecting to {}:{}",
        destination.addr,
        destination.port
    );

    let target_display = format!("{}:{}", destination.addr, destination.port);
    // Create outbound TCP connection with timeout
    // Default 15s timeout for DNS resolution + TCP handshake
    // This prevents hanging on slow/unreachable targets
    let connect_timeout = Duration::from_secs(15);
    let connect = async {
        let addresses = if let Ok(ip) = destination.addr.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, destination.port)]
        } else {
            resolve_host_addresses_with_cache(&destination.addr, destination.port).await?
        };
        connect_to_any(addresses).await.map_err(AnyTlsError::Io)
    };
    let outbound = match timeout(connect_timeout, connect).await {
        Ok(Ok(conn)) => {
            configure_tcp_stream(&conn, &target_display);
            tracing::info!("[Proxy] Successfully connected to {}", target_display);
            conn
        }
        Ok(Err(e)) => {
            tracing::error!("[Proxy] Failed to connect to {}: {}", target_display, e);
            // Send SYNACK with error if protocol version >= 2
            // Note: All streams should receive SYNACK in protocol v2+, including stream_id=1
            if peer_version >= 2 {
                let error_msg = format!("Failed to connect to {}: {}", target_display, e);
                let synack_frame =
                    Frame::with_data(Command::SynAck, stream_id, Bytes::from(error_msg));
                if let Err(send_err) = session.write_control_frame(synack_frame).await {
                    tracing::error!("[Proxy] Failed to send SYNACK with error: {}", send_err);
                }
            }
            return Err(AnyTlsError::Protocol(format!(
                "Failed to connect to {}: {}",
                target_display, e
            )));
        }
        Err(_) => {
            let error_msg = format!(
                "Connection timeout ({}s) to {}",
                connect_timeout.as_secs(),
                target_display
            );
            tracing::error!("[Proxy] {}", error_msg);
            // Send SYNACK with timeout error
            if peer_version >= 2 {
                let synack_frame =
                    Frame::with_data(Command::SynAck, stream_id, Bytes::from(error_msg.clone()));
                if let Err(send_err) = session.write_control_frame(synack_frame).await {
                    tracing::error!("[Proxy] Failed to send SYNACK with error: {}", send_err);
                }
            }
            return Err(AnyTlsError::Protocol(error_msg));
        }
    };

    // Send SYNACK after successful connection (protocol v >= 2)
    // Note: All streams should receive SYNACK in protocol v2+, including stream_id=1
    // Similar to Go's ReportHandshakeSuccess - called after TCP connection is established
    if peer_version >= 2 {
        tracing::debug!(
            "[Proxy] Sending SYNACK for stream {} (connection established)",
            stream_id
        );
        let synack_frame = Frame::control(Command::SynAck, stream_id);
        if let Err(e) = session.write_control_frame(synack_frame).await {
            tracing::error!("[Proxy] Failed to send SYNACK: {}", e);
            return Err(e);
        }
        tracing::debug!("[Proxy] SYNACK sent for stream {}", stream_id);
    }

    // Now forward data bidirectionally
    tracing::debug!(
        "[Proxy] proxy_tcp_connection_with_synack: Calling proxy_tcp_connection_data_forwarding for stream {}",
        stream_id
    );
    proxy_tcp_connection_data_forwarding(stream, outbound, destination).await
}

/// Connect to DNS candidates with a short stagger. A failed address is replaced
/// immediately; a slow address does not prevent trying later IPv4/IPv6 results.
async fn connect_to_any(addresses: Vec<SocketAddr>) -> std::io::Result<TcpStream> {
    let mut addresses = addresses.into_iter();
    let first = addresses.next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no destination addresses")
    })?;
    let mut attempts = JoinSet::new();
    attempts.spawn(async move { TcpStream::connect(first).await });
    let stagger = Duration::from_millis(250);
    let mut next_launch = Instant::now() + stagger;
    let mut last_error = None;

    loop {
        tokio::select! {
            biased;
            result = attempts.join_next() => {
                let error = match result {
                    Some(Ok(Ok(stream))) => return Ok(stream),
                    Some(Ok(Err(error))) => error,
                    Some(Err(error)) => std::io::Error::other(error),
                    None => break,
                };
                last_error = Some(error);

                if attempts.is_empty() {
                    if let Some(address) = addresses.next() {
                        attempts.spawn(async move { TcpStream::connect(address).await });
                        next_launch = Instant::now() + stagger;
                    } else {
                        break;
                    }
                }
            }
            _ = sleep_until(next_launch), if addresses.len() > 0 => {
                if let Some(address) = addresses.next() {
                    attempts.spawn(async move { TcpStream::connect(address).await });
                    next_launch += stagger;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no destination addresses")
    }))
}

/// Forward data between stream and outbound connection
///
/// 新实现：完全移除 Mutex 包装，直接使用 Stream
/// Stream 内部的 reader 和 writer 已经分离，无锁竞争
async fn proxy_tcp_connection_data_forwarding(
    stream: Arc<Stream>,
    outbound: TcpStream,
    destination: SocksAddr,
) -> Result<()> {
    let stream_id = stream.id();
    tracing::debug!(
        "[Proxy] Starting data forwarding for stream {} to {}:{}",
        stream_id,
        destination.addr,
        destination.port
    );

    // 分离 outbound 的读写
    let (mut outbound_read, mut outbound_write) = tokio::io::split(outbound);

    // ===== 关键改变：不再需要 Arc<Mutex<>> 包装！=====
    // 直接克隆 Arc<Stream> 用于两个任务
    let stream_for_read = Arc::clone(&stream);
    let stream_for_write = Arc::clone(&stream);
    let bytes_to_outbound = Arc::new(AtomicU64::new(0));
    let bytes_to_client = Arc::new(AtomicU64::new(0));

    // Task 1: Stream -> Outbound（从 stream 读取，写入 outbound）
    tracing::debug!(
        "[Proxy] Spawning Task1 (stream->outbound) for stream {}",
        stream_id
    );
    let bytes_to_outbound_clone = Arc::clone(&bytes_to_outbound);
    let mut task1 = tokio::spawn(async move {
        tracing::debug!("[Proxy-Task1] Task started for stream {}", stream_id);

        // 获取 reader 的引用（无需锁整个 stream）
        let reader_mutex = stream_for_read.reader();
        let mut buf = vec![0u8; 8192];
        let mut iteration = 0u64;

        loop {
            iteration += 1;

            // 获取 reader 的锁并读取
            // 注意：锁只在读取时持有，不影响 Task2 的写入
            let n = {
                let mut reader = reader_mutex.lock().await;
                match reader.read(&mut buf).await {
                    Ok(0) => {
                        tracing::debug!(
                            "[Proxy-Task1] Stream EOF (stream_id={}, iteration={})",
                            stream_id,
                            iteration
                        );
                        break;
                    }
                    Ok(n) => {
                        tracing::debug!(
                            "[Proxy-Task1] Read {} bytes from stream {} (iteration={})",
                            n,
                            stream_id,
                            iteration
                        );
                        n
                    }
                    Err(e) => {
                        tracing::error!(
                            "[Proxy-Task1] Stream read error (stream_id={}, iteration={}): {}",
                            stream_id,
                            iteration,
                            e
                        );
                        break;
                    }
                }
            }; // reader 锁在这里释放

            // 写入 outbound（无锁）
            if let Err(e) = outbound_write.write_all(&buf[..n]).await {
                tracing::error!("[Proxy-Task1] Outbound write error: {}", e);
                break;
            }
            bytes_to_outbound_clone.fetch_add(n as u64, Ordering::Relaxed);

            tracing::trace!(
                "[Proxy-Task1] Forwarded {} bytes to outbound (iteration={})",
                n,
                iteration
            );
        }

        tracing::debug!(
            "[Proxy-Task1] Task completed for stream {} after {} iterations",
            stream_id,
            iteration
        );
    });

    // Task 2: Outbound -> Stream（从 outbound 读取，写入 stream）
    tracing::debug!(
        "[Proxy] Spawning Task2 (outbound->stream) for stream {}",
        stream_id
    );
    let bytes_to_client_clone = Arc::clone(&bytes_to_client);
    let mut task2 = tokio::spawn(async move {
        tracing::debug!("[Proxy-Task2] Task started for stream {}", stream_id);
        let mut buf = vec![0u8; 8192];
        let mut iteration = 0u64;

        loop {
            iteration += 1;

            // 从 outbound 读取（无锁）
            let n = match outbound_read.read(&mut buf).await {
                Ok(0) => {
                    tracing::debug!(
                        "[Proxy-Task2] Outbound EOF (stream_id={}, iteration={})",
                        stream_id,
                        iteration
                    );
                    break;
                }
                Ok(n) => {
                    tracing::debug!(
                        "[Proxy-Task2] Read {} bytes from outbound (iteration={})",
                        n,
                        iteration
                    );
                    n
                }
                Err(e) => {
                    tracing::error!("[Proxy-Task2] Outbound read error: {}", e);
                    break;
                }
            };

            // 写入 stream；有界队列满时会等待消费端释放容量。
            use bytes::Bytes;
            if let Err(e) = stream_for_write
                .send_data(Bytes::copy_from_slice(&buf[..n]))
                .await
            {
                tracing::error!(
                    "[Proxy-Task2] Stream write error (stream_id={}, iteration={}): {:?}",
                    stream_id,
                    iteration,
                    e
                );
                break;
            }
            bytes_to_client_clone.fetch_add(n as u64, Ordering::Relaxed);

            tracing::trace!(
                "[Proxy-Task2] Wrote {} bytes to stream {} (iteration={})",
                n,
                stream_id,
                iteration
            );
        }

        if let Err(error) = stream_for_write.send_fin().await {
            tracing::debug!(
                stream_id,
                "[Proxy-Task2] Failed to queue stream FIN: {}",
                error
            );
        }

        tracing::debug!(
            "[Proxy-Task2] Task completed for stream {} after {} iterations",
            stream_id,
            iteration
        );
    });

    // Supervise both directions. If the client half-closes its stream, finish
    // writing its queued bytes to TCP, then allow up to one second for the TCP
    // peer's already-buffered response tail to reach the stream.
    tokio::select! {
        result = &mut task1 => {
            if let Err(error) = result {
                tracing::debug!(stream_id, "[Proxy] Stream → TCP task ended: {}", error);
            }
            if timeout(Duration::from_secs(1), &mut task2).await.is_err() {
                task2.abort();
                let _ = task2.await;
                if let Err(error) = stream.send_fin().await {
                    tracing::debug!(stream_id, "[Proxy] Failed to queue FIN after TCP tail timeout: {}", error);
                }
            }
        }
        result = &mut task2 => {
            if let Err(error) = result {
                tracing::debug!(stream_id, "[Proxy] TCP → stream task ended: {}", error);
            }
            task1.abort();
            let _ = task1.await;
        }
    }
    let outbound_bytes = bytes_to_outbound.load(Ordering::Relaxed);
    let client_bytes = bytes_to_client.load(Ordering::Relaxed);

    tracing::debug!(
        stream_id = stream_id,
        bytes_outbound = outbound_bytes,
        bytes_client = client_bytes,
        "[Proxy] Connection closed for stream {} to {}:{}",
        stream_id,
        destination.addr,
        destination.port
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn uot_magic_domains_match_exactly() {
        assert_eq!(uot_version("sp.udp-over-tcp.arpa"), Some(true));
        assert_eq!(uot_version("SP.V2.UDP-OVER-TCP.ARPA"), Some(false));
        assert_eq!(uot_version("not-sp.v2.udp-over-tcp.arpa"), None);
        assert_eq!(uot_version("sp.v2.udp-over-tcp.arpa.attacker.test"), None);
    }

    #[tokio::test]
    async fn connect_to_any_tries_later_address_after_refusal() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let expected = listener.local_addr().unwrap();
        let refused = "127.0.0.1:0".parse().unwrap();

        let stream = timeout(
            Duration::from_secs(2),
            connect_to_any(vec![refused, expected]),
        )
        .await
        .expect("connection attempts should finish promptly")
        .expect("the later address should accept the connection");

        assert_eq!(stream.peer_addr().unwrap(), expected);
    }
}
