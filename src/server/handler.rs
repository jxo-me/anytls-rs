//! Server connection handlers

use crate::session::{Stream, Session};
use crate::util::{AnyTlsError, Result};
use crate::protocol::{Frame, Command};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use bytes::Bytes;

/// Handler trait for processing new streams
pub trait StreamHandler: Send + Sync {
    fn handle_stream(&self, stream: Arc<Stream>, session: Arc<crate::session::Session>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;
}

/// Default stream handler that proxies TCP connections
pub struct TcpProxyHandler {
    // Destination will be read from stream
}

impl TcpProxyHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl StreamHandler for TcpProxyHandler {
    fn handle_stream(&self, stream: Arc<Stream>, session: Arc<crate::session::Session>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let stream_id = stream.id();
            let peer_version = session.peer_version();
            tracing::debug!("[Proxy] Handling stream {} (peer_version={})", stream_id, peer_version);
            
            // Proxy the connection (reads SOCKS5 address and establishes outbound TCP connection)
            // Similar to Go's proxyOutboundTCP - send SYNACK after TCP connection is established
            proxy_tcp_connection_with_synack(stream, session, stream_id, peer_version).await
        })
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
/// Uses Mutex to get mutable access to Stream for AsyncReadExt methods
async fn read_socks_addr(stream: Arc<Stream>) -> Result<SocksAddr> {
    let stream_id = stream.id();
    tracing::info!("[Proxy] 📖 read_socks_addr: Starting to read SOCKS5 address from stream {}", stream_id);
    
    // We can't unwrap Arc because Session also holds a reference
    // Wrap Arc<Stream> in Mutex for safe mutable access
    let stream_mutex = Arc::new(tokio::sync::Mutex::new(stream));
    
    // Read ATYP byte first
    tracing::info!("[Proxy] 📖 read_socks_addr: Reading ATYP byte from stream {}", stream_id);
    let mut atyp_buf = [0u8; 1];
    {
        tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for ATYP (stream {})", stream_id);
        let stream_arc = stream_mutex.lock().await;
        use std::pin::Pin;
        let stream_ptr = Arc::as_ptr(&stream_arc);
        let stream_mut_ptr = stream_ptr as *mut Stream;
        let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
        let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
        AsyncReadExt::read_exact(&mut pinned, &mut atyp_buf).await
            .map_err(|e| AnyTlsError::Protocol(format!("Failed to read address type: {}", e)))?;
    }
    tracing::info!("[Proxy] ✅ read_socks_addr: Read ATYP={:02x} from stream {}", atyp_buf[0], stream_id);
    
    let atyp = atyp_buf[0];
    let addr = match atyp {
        0x01 => {
            // IPv4: 4 bytes
            tracing::info!("[Proxy] 📖 read_socks_addr: Reading IPv4 address (stream {})", stream_id);
            let mut ip_buf = [0u8; 4];
            {
                tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for IPv4 (stream {})", stream_id);
                let stream_arc = stream_mutex.lock().await;
                use std::pin::Pin;
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
                AsyncReadExt::read_exact(&mut pinned, &mut ip_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read IPv4: {}", e)))?;
            }
            IpAddr::V4(Ipv4Addr::from(ip_buf)).to_string()
        }
        0x03 => {
            // Domain name: [LEN (1 byte) | DOMAIN (LEN bytes)]
            tracing::info!("[Proxy] 📖 read_socks_addr: Reading domain name (stream {})", stream_id);
            let mut len_buf = [0u8; 1];
            {
                tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for domain length (stream {})", stream_id);
                let stream_arc = stream_mutex.lock().await;
                use std::pin::Pin;
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
                AsyncReadExt::read_exact(&mut pinned, &mut len_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read domain length: {}", e)))?;
            }
            
            let domain_len = len_buf[0] as usize;
            tracing::info!("[Proxy] 📖 read_socks_addr: Domain length={} (stream {})", domain_len, stream_id);
            if domain_len == 0 || domain_len > 255 {
                return Err(AnyTlsError::Protocol("Invalid domain length".to_string()));
            }
            
            let mut domain_buf = vec![0u8; domain_len];
            {
                tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for domain (stream {})", stream_id);
                let stream_arc = stream_mutex.lock().await;
                use std::pin::Pin;
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
                AsyncReadExt::read_exact(&mut pinned, &mut domain_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read domain: {}", e)))?;
            }
            
            String::from_utf8(domain_buf)
                .map_err(|e| AnyTlsError::Protocol(format!("Invalid domain name: {}", e)))?
        }
        0x04 => {
            // IPv6: 16 bytes
            tracing::info!("[Proxy] 📖 read_socks_addr: Reading IPv6 address (stream {})", stream_id);
            let mut ip_buf = [0u8; 16];
            {
                tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for IPv6 (stream {})", stream_id);
                let stream_arc = stream_mutex.lock().await;
                use std::pin::Pin;
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
                AsyncReadExt::read_exact(&mut pinned, &mut ip_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read IPv6: {}", e)))?;
            }
            IpAddr::V6(Ipv6Addr::from(ip_buf)).to_string()
        }
        _ => {
            return Err(AnyTlsError::Protocol(format!("Unsupported address type: 0x{:02x}", atyp)));
        }
    };
    
    // Read port (2 bytes, big-endian)
    tracing::info!("[Proxy] 📖 read_socks_addr: Reading port (stream {})", stream_id);
    let mut port_buf = [0u8; 2];
    {
        tracing::trace!("[Proxy] read_socks_addr: Acquiring stream lock for port (stream {})", stream_id);
        let stream_arc = stream_mutex.lock().await;
        use std::pin::Pin;
        let stream_ptr = Arc::as_ptr(&stream_arc);
        let stream_mut_ptr = stream_ptr as *mut Stream;
        let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
        let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
        AsyncReadExt::read_exact(&mut pinned, &mut port_buf).await
            .map_err(|e| AnyTlsError::Protocol(format!("Failed to read port: {}", e)))?;
    }
    let port = u16::from_be_bytes(port_buf);
    
    tracing::info!("[Proxy] ✅ read_socks_addr: Successfully read address {}:{} from stream {}", addr, port, stream_id);
    Ok(SocksAddr { addr, port })
}

/// Proxy TCP connection with SYNACK support: read destination from stream, establish connection, send SYNACK, and forward data
async fn proxy_tcp_connection_with_synack(
    stream: Arc<Stream>,
    session: Arc<Session>,
    stream_id: u32,
    peer_version: u8,
) -> Result<()> {
    tracing::info!("[Proxy] 🚀 proxy_tcp_connection_with_synack: Starting for stream {} (peer_version={})", stream_id, peer_version);
    
    // Read destination address from stream (SOCKS5 format)
    let destination = read_socks_addr(Arc::clone(&stream)).await?;
    
    tracing::info!("[Proxy] 🔗 Connecting to {}:{}", destination.addr, destination.port);
    
    // Resolve destination address
    let target_addr = format!("{}:{}", destination.addr, destination.port);
    
    // Create outbound TCP connection
    let outbound = match TcpStream::connect(&target_addr).await {
        Ok(conn) => {
            tracing::info!("[Proxy] ✅ Successfully connected to {}:{}", destination.addr, destination.port);
            conn
        }
        Err(e) => {
            tracing::error!("[Proxy] ❌ Failed to connect to {}: {}", target_addr, e);
            // Send SYNACK with error if protocol version >= 2
            if peer_version >= 2 && stream_id >= 2 {
                let error_msg = format!("Failed to connect to {}: {}", target_addr, e);
                let synack_frame = Frame::with_data(
                    Command::SynAck,
                    stream_id,
                    Bytes::from(error_msg),
                );
                if let Err(send_err) = session.write_control_frame(synack_frame).await {
                    tracing::error!("[Proxy] Failed to send SYNACK with error: {}", send_err);
                }
            }
            return Err(AnyTlsError::Protocol(format!("Failed to connect to {}: {}", target_addr, e)));
        }
    };
    
    // Send SYNACK after successful connection (protocol v >= 2 and stream_id >= 2)
    // Similar to Go's ReportHandshakeSuccess - called after TCP connection is established
    if peer_version >= 2 && stream_id >= 2 {
        tracing::info!("[Proxy] 📤 Sending SYNACK for stream {} (connection established)", stream_id);
        let synack_frame = Frame::control(Command::SynAck, stream_id);
        if let Err(e) = session.write_control_frame(synack_frame).await {
            tracing::error!("[Proxy] Failed to send SYNACK: {}", e);
            return Err(e);
        }
        tracing::info!("[Proxy] ✅ SYNACK sent for stream {}", stream_id);
    }
    
    // Now forward data bidirectionally
    tracing::info!("[Proxy] 🔄 proxy_tcp_connection_with_synack: Calling proxy_tcp_connection_data_forwarding for stream {}", stream_id);
    proxy_tcp_connection_data_forwarding(stream, outbound, destination).await
}

/// Forward data between stream and outbound connection
async fn proxy_tcp_connection_data_forwarding(
    stream: Arc<Stream>,
    outbound: TcpStream,
    destination: SocksAddr,
) -> Result<()> {
    let stream_id_for_log = stream.id();
    tracing::info!("[Proxy] Starting data forwarding for stream {} to {}:{}", 
        stream_id_for_log, destination.addr, destination.port);
    
    // Split the outbound connection for bidirectional copying
    let (mut outbound_read, mut outbound_write) = tokio::io::split(outbound);
    
    // Wrap Arc<Stream> in Mutex for safe mutable access
    let stream_mutex = Arc::new(tokio::sync::Mutex::new(stream));
    let stream_read = Arc::clone(&stream_mutex);
    let stream_write = Arc::clone(&stream_mutex);
    
    // Perform bidirectional data forwarding
    // Task 1: Stream -> Outbound
    tracing::debug!("[Proxy] Spawning Task1 (stream->outbound) for stream {}", stream_id_for_log);
    let task1 = tokio::spawn(async move {
        tracing::debug!("[Proxy-Task1] Task spawned, starting stream->outbound forwarding for stream {}", stream_id_for_log);
        tokio::task::yield_now().await;
        let mut buf = vec![0u8; 8192];
        let mut iteration = 0u64;
        loop {
            iteration += 1;
            tracing::trace!("[Proxy-Task1] Iteration {}: Reading from stream {}", iteration, stream_id_for_log);
            
            // Use timeout to periodically release lock and allow Task2 to acquire it
            let n = tokio::time::timeout(std::time::Duration::from_millis(100), async {
                tracing::trace!("[Proxy-Task1] Acquiring stream lock to check buffer (iteration {})", iteration);
                let mut stream_arc = stream_read.lock().await;
                // Get mutable reference to Stream (unsafe because we hold the Mutex)
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                
                // Try to read from buffer first (non-blocking)
                let buffer_read = stream_ref.try_read_from_buffer(&mut buf);
                
                if buffer_read > 0 {
                    drop(stream_arc); // Release lock immediately
                    tracing::debug!("[Proxy-Task1] Read {} bytes from buffer (iteration {})", buffer_read, iteration);
                    return Ok::<usize, ()>(buffer_read);
                }
                
                // Buffer is empty, need to wait for channel data
                // Use timeout to prevent holding lock too long
                tracing::trace!("[Proxy-Task1] Buffer empty, waiting for channel data (iteration {})", iteration);
                match tokio::time::timeout(std::time::Duration::from_millis(50), stream_ref.recv_from_channel()).await {
                    Ok(Some(data)) => {
                        let n = stream_ref.store_in_buffer(data, &mut buf);
                        drop(stream_arc); // Release lock
                        tracing::debug!("[Proxy-Task1] Received and stored {} bytes from channel (iteration {})", n, iteration);
                        Ok(n)
                    }
                    Ok(None) => {
                        drop(stream_arc); // Release lock
                        tracing::debug!("[Proxy-Task1] Channel closed (EOF) (iteration {})", iteration);
                        Err(())
                    }
                    Err(_) => {
                        // Timeout - release lock and retry
                        drop(stream_arc);
                        tracing::trace!("[Proxy-Task1] Channel wait timeout, releasing lock (iteration {})", iteration);
                        Err(())
                    }
                }
            }).await;
            
            let n = match n {
                Ok(Ok(n)) => n,
                Ok(Err(_)) => {
                    // EOF or error
                    tracing::debug!("[Proxy-Task1] Stream read EOF (iteration {})", iteration);
                    break;
                }
                Err(_) => {
                    // Outer timeout - retry the loop (this allows Task2 to get lock)
                    tracing::trace!("[Proxy-Task1] Lock acquisition timeout, retrying (iteration {})", iteration);
                    tokio::task::yield_now().await; // Yield to allow Task2 to run
                    continue;
                }
            };
            
            tracing::debug!("[Proxy-Task1] Writing {} bytes to outbound (iteration {})", n, iteration);
            if outbound_write.write_all(&buf[..n]).await.is_err() {
                tracing::error!("[Proxy-Task1] Error writing {} bytes to outbound (iteration {})", n, iteration);
                break;
            }
            tracing::trace!("[Proxy-Task1] Forwarded {} bytes to outbound (iteration {})", n, iteration);
        }
        tracing::debug!("[Proxy-Task1] Task1 finished for stream {} after {} iterations", stream_id_for_log, iteration);
    });
    
    // Task 2: Outbound -> Stream
    tracing::debug!("[Proxy] Spawning Task2 (outbound->stream) for stream {}", stream_id_for_log);
    let task2 = tokio::spawn(async move {
        tracing::debug!("[Proxy-Task2] Task spawned, starting outbound->stream forwarding for stream {}", stream_id_for_log);
        tokio::task::yield_now().await;
        let mut buf = vec![0u8; 8192];
        let mut iteration = 0u64;
        loop {
            iteration += 1;
            tracing::trace!("[Proxy-Task2] Iteration {}: Reading from outbound", iteration);
            let n = match outbound_read.read(&mut buf).await {
                Ok(0) => {
                    tracing::debug!("[Proxy-Task2] Outbound read EOF (iteration {})", iteration);
                    break;
                }
                Ok(n) => {
                    tracing::debug!("[Proxy-Task2] Read {} bytes from outbound (iteration {})", n, iteration);
                    n
                }
                Err(e) => {
                    tracing::error!("[Proxy-Task2] Error reading from outbound: {} (iteration {})", e, iteration);
                    break;
                }
            };
            tracing::trace!("[Proxy-Task2] Acquiring stream lock for write (iteration {})", iteration);
            let lock_result = tokio::time::timeout(std::time::Duration::from_secs(5), stream_write.lock()).await;
            let stream_arc = match lock_result {
                Ok(guard) => {
                    tracing::trace!("[Proxy-Task2] Stream lock acquired (iteration {})", iteration);
                    guard
                }
                Err(_) => {
                    tracing::error!("[Proxy-Task2] Timeout waiting for stream lock (iteration {})", iteration);
                    break;
                }
            };
            {
                use std::pin::Pin;
                let stream_ptr = Arc::as_ptr(&stream_arc);
                let stream_mut_ptr = stream_ptr as *mut Stream;
                let stream_ref: &mut Stream = unsafe { &mut *stream_mut_ptr };
                let mut pinned = unsafe { Pin::new_unchecked(stream_ref) };
                tracing::trace!("[Proxy-Task2] Calling AsyncWriteExt::write_all for {} bytes (iteration {})", n, iteration);
                let write_result = AsyncWriteExt::write_all(&mut pinned, &buf[..n]).await;
                tracing::trace!("[Proxy-Task2] AsyncWriteExt::write_all returned: {:?} (iteration {})", write_result.as_ref().map(|_| "Ok").map_err(|e| e), iteration);
                if write_result.is_err() {
                    tracing::error!("[Proxy-Task2] Error writing {} bytes to stream {} (iteration {})", n, stream_id_for_log, iteration);
                    break;
                }
                tracing::debug!("[Proxy-Task2] Wrote {} bytes to stream {} (iteration {})", n, stream_id_for_log, iteration);
            }
            drop(stream_arc);
            // Lock is released here
            tracing::trace!("[Proxy-Task2] Stream lock released (iteration {})", iteration);
        }
        tracing::debug!("[Proxy-Task2] Task2 finished for stream {} after {} iterations", stream_id_for_log, iteration);
    });
    
    // Wait for both directions to complete
    tracing::debug!("[Proxy] Waiting for Task1 and Task2 to complete for stream {}", stream_id_for_log);
    let _ = task1.await;
    tracing::debug!("[Proxy] Task1 completed for stream {}", stream_id_for_log);
    let _ = task2.await;
    tracing::debug!("[Proxy] Task2 completed for stream {}", stream_id_for_log);
    
    tracing::info!("[Proxy] Connection to {}:{} closed for stream {}", destination.addr, destination.port, stream_id_for_log);
    
    Ok(())
}
