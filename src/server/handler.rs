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
/// 
/// 新实现：直接使用 StreamReader，无需额外的 Mutex 包装
async fn read_socks_addr(stream: Arc<Stream>) -> Result<SocksAddr> {
    let stream_id = stream.id();
    tracing::info!("[Proxy] 📖 read_socks_addr: Starting to read SOCKS5 address from stream {}", stream_id);
    
    // 获取 reader 的引用
    let reader_mutex = stream.reader();
    
    // Read ATYP byte first
    tracing::info!("[Proxy] 📖 read_socks_addr: Reading ATYP byte from stream {}", stream_id);
    let mut atyp_buf = [0u8; 1];
    {
        let mut reader = reader_mutex.lock().await;
        reader.read(&mut atyp_buf).await
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
                let mut reader = reader_mutex.lock().await;
                reader.read_exact(&mut ip_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read IPv4: {}", e)))?;
            }
            IpAddr::V4(Ipv4Addr::from(ip_buf)).to_string()
        }
        0x03 => {
            // Domain name: [LEN (1 byte) | DOMAIN (LEN bytes)]
            tracing::info!("[Proxy] 📖 read_socks_addr: Reading domain name (stream {})", stream_id);
            let mut len_buf = [0u8; 1];
            {
                let mut reader = reader_mutex.lock().await;
                reader.read_exact(&mut len_buf).await
                    .map_err(|e| AnyTlsError::Protocol(format!("Failed to read domain length: {}", e)))?;
            }
            
            let domain_len = len_buf[0] as usize;
            tracing::info!("[Proxy] 📖 read_socks_addr: Domain length={} (stream {})", domain_len, stream_id);
            if domain_len == 0 || domain_len > 255 {
                return Err(AnyTlsError::Protocol("Invalid domain length".to_string()));
            }
            
            let mut domain_buf = vec![0u8; domain_len];
            {
                let mut reader = reader_mutex.lock().await;
                reader.read_exact(&mut domain_buf).await
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
                let mut reader = reader_mutex.lock().await;
                reader.read_exact(&mut ip_buf).await
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
        let mut reader = reader_mutex.lock().await;
        reader.read_exact(&mut port_buf).await
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
/// 
/// 新实现：完全移除 Mutex 包装，直接使用 Stream
/// Stream 内部的 reader 和 writer 已经分离，无锁竞争
async fn proxy_tcp_connection_data_forwarding(
    stream: Arc<Stream>,
    outbound: TcpStream,
    destination: SocksAddr,
) -> Result<()> {
    let stream_id = stream.id();
    tracing::info!("[Proxy] Starting data forwarding for stream {} to {}:{}", 
        stream_id, destination.addr, destination.port);
    
    // 分离 outbound 的读写
    let (mut outbound_read, mut outbound_write) = tokio::io::split(outbound);
    
    // ===== 关键改变：不再需要 Arc<Mutex<>> 包装！=====
    // 直接克隆 Arc<Stream> 用于两个任务
    let stream_for_read = Arc::clone(&stream);
    let stream_for_write = Arc::clone(&stream);
    
    // Task 1: Stream -> Outbound（从 stream 读取，写入 outbound）
    tracing::debug!("[Proxy] Spawning Task1 (stream->outbound) for stream {}", stream_id);
    let task1 = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        
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
                        tracing::debug!("[Proxy-Task1] Stream EOF (stream_id={}, iteration={})", stream_id, iteration);
                        break;
                    }
                    Ok(n) => {
                        tracing::debug!("[Proxy-Task1] Read {} bytes from stream {} (iteration={})", n, stream_id, iteration);
                        n
                    }
                    Err(e) => {
                        tracing::error!("[Proxy-Task1] Stream read error (stream_id={}, iteration={}): {}", stream_id, iteration, e);
                        break;
                    }
                }
            }; // reader 锁在这里释放
            
            // 写入 outbound（无锁）
            if let Err(e) = outbound_write.write_all(&buf[..n]).await {
                tracing::error!("[Proxy-Task1] Outbound write error: {}", e);
                break;
            }
            
            tracing::trace!("[Proxy-Task1] Forwarded {} bytes to outbound (iteration={})", n, iteration);
        }
        
        tracing::debug!("[Proxy-Task1] Task completed for stream {} after {} iterations", stream_id, iteration);
    });
    
    // Task 2: Outbound -> Stream（从 outbound 读取，写入 stream）
    tracing::debug!("[Proxy] Spawning Task2 (outbound->stream) for stream {}", stream_id);
    let task2 = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        
        tracing::debug!("[Proxy-Task2] Task started for stream {}", stream_id);
        let mut buf = vec![0u8; 8192];
        let mut iteration = 0u64;
        
        loop {
            iteration += 1;
            
            // 从 outbound 读取（无锁）
            let n = match outbound_read.read(&mut buf).await {
                Ok(0) => {
                    tracing::debug!("[Proxy-Task2] Outbound EOF (stream_id={}, iteration={})", stream_id, iteration);
                    break;
                }
                Ok(n) => {
                    tracing::debug!("[Proxy-Task2] Read {} bytes from outbound (iteration={})", n, iteration);
                    n
                }
                Err(e) => {
                    tracing::error!("[Proxy-Task2] Outbound read error: {}", e);
                    break;
                }
            };
            
            // 写入 stream（使用 send_data，完全无锁！）
            use bytes::Bytes;
            if let Err(e) = stream_for_write.send_data(Bytes::copy_from_slice(&buf[..n])) {
                tracing::error!("[Proxy-Task2] Stream write error (stream_id={}, iteration={}): {:?}", stream_id, iteration, e);
                break;
            }
            
            tracing::trace!("[Proxy-Task2] Wrote {} bytes to stream {} (iteration={})", n, stream_id, iteration);
        }
        
        tracing::debug!("[Proxy-Task2] Task completed for stream {} after {} iterations", stream_id, iteration);
    });
    
    // 等待两个任务完成
    tracing::debug!("[Proxy] Waiting for tasks to complete for stream {}", stream_id);
    let _ = tokio::join!(task1, task2);
    
    tracing::info!("[Proxy] Connection closed for stream {} to {}:{}", 
        stream_id, destination.addr, destination.port);
    
    Ok(())
}
