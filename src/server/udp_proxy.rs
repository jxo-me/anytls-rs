//! UDP over TCP proxy implementation
//!
//! Implements sing-box udp-over-tcp v2 protocol (Connect format)
//! 
//! # Protocol Format
//! 
//! ## Request (sent once at stream start):
//! ```text
//! | isConnect | ATYP | Address | Port |
//! | u8 (=1)   | u8   | variable| u16be|
//! ```
//! 
//! ## Data packets (Connect format, isConnect=1):
//! ```text
//! | Length | Data     |
//! | u16be  | variable |
//! ```
//! 
//! Reference: <https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md>

use crate::session::{Stream, StreamReader};
use crate::util::{AnyTlsError, Result};
use bytes::{Bytes, BytesMut, BufMut};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

const MAX_UDP_PACKET_SIZE: usize = 65535;

/// Handle UDP over TCP stream
/// 
/// Target address should be "sp.v2.udp-over-tcp.arpa"
/// 
/// # Protocol
/// 
/// sing-box udp-over-tcp v2:
/// 1. First, read initial request containing target address (SOCKS5 format)
/// 2. Then, each packet: Length (2 bytes BE) + Payload
/// 3. Bidirectional forwarding between Stream and UDP socket
/// 
/// Reference: https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md
pub async fn handle_udp_over_tcp(
    stream: Arc<Stream>,
) -> Result<()> {
    let stream_id = stream.id();
    
    tracing::info!("[UDP] 🔵 Starting UDP over TCP proxy for stream {}", stream_id);
    
    let reader = stream.reader();
    let mut reader_guard = reader.lock().await;
    
    // Step 1: Read initial request (contains target address)
    // Format: SOCKS5 address (ATYP + Address + Port)
    let target_addr = match read_initial_request(&mut *reader_guard).await {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("[UDP] Failed to read initial request: {}", e);
            return Err(e);
        }
    };
    
    tracing::info!("[UDP] Target UDP address: {}", target_addr);
    
    drop(reader_guard);
    
    // Step 2: Create UDP socket (bind to any available port)
    let udp_socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
        tracing::error!("[UDP] Failed to create UDP socket: {}", e);
        AnyTlsError::Io(e)
    })?;
    
    let local_addr = udp_socket.local_addr()?;
    tracing::info!("[UDP] Created UDP socket on {}", local_addr);
    
    // Step 3: Send handshake success (if needed, similar to Go's ReportHandshakeSuccess)
    // In our case, we can just start forwarding
    
    // Step 4: Bidirectional forwarding
    tokio::select! {
        result = stream_to_udp(&stream, &udp_socket, &target_addr) => {
            if let Err(e) = result {
                tracing::error!("[UDP] Stream → UDP error: {}", e);
                return Err(e);
            }
        }
        result = udp_to_stream(&stream, &udp_socket, &target_addr) => {
            if let Err(e) = result {
                tracing::error!("[UDP] UDP → Stream error: {}", e);
                return Err(e);
            }
        }
    }
    
    tracing::info!("[UDP] UDP over TCP proxy completed for stream {}", stream_id);
    Ok(())
}

/// Read initial request from stream
/// 
/// Format (sing-box udp-over-tcp v2 request):
/// ```text
/// | isConnect | ATYP | Address | Port |
/// | u8        | u8   | variable| u16be|
/// ```
/// 
/// Returns the target SocketAddr
async fn read_initial_request(reader: &mut StreamReader) -> Result<SocketAddr> {
    use tokio::io::AsyncReadExt as _;
    
    // Read isConnect (1 byte)
    let mut is_connect_buf = [0u8; 1];
    reader.read_exact(&mut is_connect_buf).await.map_err(|e| {
        AnyTlsError::Io(e)
    })?;
    
    let is_connect = is_connect_buf[0];
    
    if is_connect != 1 {
        // We only support connect format (isConnect=1)
        return Err(AnyTlsError::Protocol(
            format!("Unsupported UDP over TCP format: isConnect={}", is_connect)
        ));
    }
    
    tracing::debug!("[UDP] Using Connect format (isConnect=1)");
    
    // Read ATYP (1 byte)
    let mut atyp_buf = [0u8; 1];
    reader.read_exact(&mut atyp_buf).await.map_err(|e| {
        AnyTlsError::Io(e)
    })?;
    
    let atyp = atyp_buf[0];
    
    match atyp {
        0x01 => {
            // IPv4: 4 bytes IP + 2 bytes port
            let mut ip_buf = [0u8; 4];
            reader.read_exact(&mut ip_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let mut port_buf = [0u8; 2];
            reader.read_exact(&mut port_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let ip = std::net::Ipv4Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);
            
            Ok(SocketAddr::from((ip, port)))
        }
        0x04 => {
            // IPv6: 16 bytes IP + 2 bytes port
            let mut ip_buf = [0u8; 16];
            reader.read_exact(&mut ip_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let mut port_buf = [0u8; 2];
            reader.read_exact(&mut port_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let ip = std::net::Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);
            
            Ok(SocketAddr::from((ip, port)))
        }
        0x03 => {
            // Domain: length (1 byte) + domain + 2 bytes port
            let mut len_buf = [0u8; 1];
            reader.read_exact(&mut len_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let domain_len = len_buf[0] as usize;
            if domain_len == 0 || domain_len > 255 {
                return Err(AnyTlsError::Protocol("Invalid domain length".into()));
            }
            
            let mut domain_buf = vec![0u8; domain_len];
            reader.read_exact(&mut domain_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let domain = String::from_utf8(domain_buf)
                .map_err(|e| AnyTlsError::Protocol(format!("Invalid domain name: {}", e)))?;
            
            let mut port_buf = [0u8; 2];
            reader.read_exact(&mut port_buf).await.map_err(|e| {
                AnyTlsError::Io(e)
            })?;
            
            let port = u16::from_be_bytes(port_buf);
            
            // Resolve domain name
            // Note: In a production environment, we might want to cache DNS results
            let addr = tokio::net::lookup_host((domain.as_str(), port))
                .await
                .map_err(|e| AnyTlsError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("DNS resolution failed for {}: {}", domain, e)
                )))?
                .next()
                .ok_or_else(|| AnyTlsError::Protocol(format!("No address found for {}", domain)))?;
            
            Ok(addr)
        }
        _ => {
            Err(AnyTlsError::Protocol(
                format!("Unknown address type: {}", atyp)
            ))
        }
    }
}

/// Stream → UDP: Read packets from Stream, decode and send to UDP
/// 
/// Protocol: Each packet is Length (2 bytes BE) + Payload
/// The payload is pure UDP data (target address already known from initial request)
async fn stream_to_udp(
    stream: &Stream,
    udp: &UdpSocket,
    target_addr: &SocketAddr,
) -> Result<()> {
    let stream_id = stream.id();
    let reader = stream.reader();
    let mut reader_guard = reader.lock().await;
    
    tracing::debug!("[UDP] Stream → UDP task started for stream {}", stream_id);
    
    loop {
        // Read one UDP packet (Length + Payload format)
        let payload = match read_udp_packet(&mut *reader_guard).await {
            Ok(data) => data,
            Err(e) => {
                if e.to_string().contains("UnexpectedEof") || e.to_string().contains("EOF") {
                    tracing::debug!("[UDP] Stream closed (EOF), stopping Stream → UDP");
                    break;
                }
                tracing::error!("[UDP] Failed to read UDP packet from stream: {}", e);
                return Err(e);
            }
        };
        
        if payload.is_empty() {
            tracing::debug!("[UDP] Empty packet, stream might be closed");
            break;
        }
        
        tracing::trace!(
            "[UDP] Stream → UDP: {} bytes to {}",
            payload.len(),
            target_addr
        );
        
        // Send to UDP (target address already known from initial request)
        let sent = udp.send_to(&payload, target_addr).await?;
        
        if sent != payload.len() {
            tracing::warn!(
                "[UDP] Partial UDP send: {} / {} bytes",
                sent,
                payload.len()
            );
        }
    }
    
    Ok(())
}

/// UDP → Stream: Read from UDP, encode and send to Stream
/// 
/// Protocol: Each packet is Length (2 bytes BE) + Payload
/// The payload is pure UDP data (source address info might be embedded, but for simplicity
/// we just send the data since the connection is already established)
async fn udp_to_stream(
    stream: &Stream,
    udp: &UdpSocket,
    _target_addr: &SocketAddr,
) -> Result<()> {
    let stream_id = stream.id();
    
    tracing::debug!("[UDP] UDP → Stream task started for stream {}", stream_id);
    
    let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
    
    loop {
        // Receive from UDP
        let (len, addr) = match udp.recv_from(&mut buf).await {
            Ok((len, addr)) => (len, addr),
            Err(e) => {
                tracing::error!("[UDP] Failed to receive from UDP: {}", e);
                return Err(AnyTlsError::Io(e));
            }
        };
        
        tracing::trace!("[UDP] UDP → Stream: {} bytes from {}", len, addr);
        
        // Encode: Length (2 bytes BE) + Payload
        // Note: According to sing-box protocol, the payload is just the UDP data
        // The source address info is not included in each packet (connection is established)
        let packet = encode_udp_packet_simple(&buf[..len])?;
        
        // Send to Stream using the send_data method
        if let Err(e) = stream.send_data(packet) {
            tracing::error!("[UDP] Failed to send to stream: {}", e);
            return Err(AnyTlsError::Protocol("Channel send failed".into()));
        }
    }
}

/// Read one UDP packet from Stream
/// 
/// Format: | Length (2 bytes BE) | Payload |
/// Returns the payload (without length prefix)
async fn read_udp_packet(reader: &mut StreamReader) -> Result<Vec<u8>> {
    // Read 2-byte length (Big-Endian)
    let mut len_buf = [0u8; 2];
    reader.read_exact(&mut len_buf).await.map_err(|e| {
        AnyTlsError::Io(e)
    })?;
    
    let len = u16::from_be_bytes(len_buf) as usize;
    
    if len == 0 {
        return Ok(Vec::new());
    }
    
    if len > MAX_UDP_PACKET_SIZE {
        return Err(AnyTlsError::Protocol(
            format!("UDP packet too large: {} bytes", len)
        ));
    }
    
    // Read the actual payload
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).await.map_err(|e| {
        AnyTlsError::Io(e)
    })?;
    
    Ok(data)
}

/// Encode UDP packet (simple format)
/// 
/// Format (sing-box v2 after initial request):
/// | Length (2 bytes BE) | Payload |
/// 
/// The payload is pure UDP data (no address encoding needed)
fn encode_udp_packet_simple(payload: &[u8]) -> Result<Bytes> {
    let mut buf = BytesMut::new();
    
    if payload.len() > MAX_UDP_PACKET_SIZE {
        return Err(AnyTlsError::Protocol(
            format!("UDP packet too large: {} bytes", payload.len())
        ));
    }
    
    // Write length (2 bytes, Big-Endian)
    buf.put_u16(payload.len() as u16);
    
    // Write payload
    buf.put_slice(payload);
    
    Ok(buf.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_udp_packet_length() {
        // Test that we can encode and decode length correctly
        let payload = b"Test UDP data";
        let encoded = encode_udp_packet_simple(payload).unwrap();
        
        // Check length prefix
        let len = u16::from_be_bytes([encoded[0], encoded[1]]) as usize;
        assert_eq!(len, payload.len());
        assert_eq!(encoded.len(), 2 + payload.len());
        
        // Check payload
        assert_eq!(&encoded[2..], payload);
    }
    
    #[test]
    fn test_encode_empty_packet() {
        let payload = b"";
        let encoded = encode_udp_packet_simple(payload).unwrap();
        
        // Should have 2-byte length header with value 0
        assert_eq!(encoded.len(), 2);
        assert_eq!(u16::from_be_bytes([encoded[0], encoded[1]]), 0);
    }
    
    #[test]
    fn test_encode_large_packet() {
        let payload = vec![0u8; 65535]; // Max UDP packet size
        let result = encode_udp_packet_simple(&payload);
        assert!(result.is_ok());
        
        let encoded = result.unwrap();
        assert_eq!(encoded.len(), 2 + 65535);
        assert_eq!(u16::from_be_bytes([encoded[0], encoded[1]]), 65535);
    }
    
    #[test]
    fn test_encode_too_large_packet() {
        let payload = vec![0u8; 65536]; // Too large
        let result = encode_udp_packet_simple(&payload);
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(e.to_string().contains("too large"));
        }
    }
}

