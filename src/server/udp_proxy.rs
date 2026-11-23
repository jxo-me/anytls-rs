//! UDP over TCP proxy implementation
//!
//! Implements sing-box udp-over-tcp v2 protocol (Connect and non-Connect formats)
//!
//! # Protocol Format
//!
//! ## Request (sent once at stream start):
//! ```text
//! | isConnect | ATYP | Address | Port |
//! | u8 (=1)   | u8   | variable| u16be|
//! ```
//!
//! ## Connect data packets (isConnect=1):
//! ```text
//! | Length | Data     |
//! | u16be  | variable |
//! ```
//!
//! Non-Connect packets (isConnect=0) use the v1 per-packet address format.
//!
//! Reference: <https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md>

use crate::session::{Stream, StreamReader};
use crate::util::{AnyTlsError, Result, resolve_host_with_cache};
use bytes::{BufMut, Bytes, BytesMut};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UdpSocket;
use tracing::{field, info_span};

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
/// Reference: <https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md>
pub async fn handle_udp_over_tcp(stream: Arc<Stream>) -> Result<()> {
    let stream_id = stream.id();
    let udp_span = info_span!(
        "anytls.udp.proxy",
        stream_id,
        local_udp = field::Empty,
        target = field::Empty,
        packets_in = field::Empty,
        packets_out = field::Empty,
        bytes_in = field::Empty,
        bytes_out = field::Empty
    );
    let _udp_guard = udp_span.enter();

    tracing::debug!("[UDP] Starting UDP over TCP proxy for stream {}", stream_id);

    let reader = stream.reader();
    let mut reader_guard = reader.lock().await;

    // Step 1: Read initial request (contains target address)
    // Format: SOCKS5 address (ATYP + Address + Port)
    let (is_connect, target_addr) = match read_initial_request(&mut reader_guard).await {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("[UDP] Failed to read initial request: {}", e);
            return Err(e);
        }
    };
    udp_span.record("target", field::display(target_addr));

    tracing::debug!("[UDP] Target UDP address: {}", target_addr);

    drop(reader_guard);

    // Step 2: Create UDP socket (bind to any available port)
    let udp_socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
        tracing::error!("[UDP] Failed to create UDP socket: {}", e);
        AnyTlsError::Io(e)
    })?;

    let local_addr = udp_socket.local_addr()?;
    tracing::debug!("[UDP] Created UDP socket on {}", local_addr);
    udp_span.record("local_udp", field::display(local_addr));

    let packets_stream_to_udp = Arc::new(AtomicU64::new(0));
    let bytes_stream_to_udp = Arc::new(AtomicU64::new(0));
    let packets_udp_to_stream = Arc::new(AtomicU64::new(0));
    let bytes_udp_to_stream = Arc::new(AtomicU64::new(0));

    // Step 3: Send handshake success (if needed, similar to Go's ReportHandshakeSuccess)
    // In our case, we can just start forwarding

    // Step 4: Bidirectional forwarding
    tokio::select! {
        result = stream_to_udp(
            &stream,
            &udp_socket,
            is_connect,
            &target_addr,
            Arc::clone(&packets_stream_to_udp),
            Arc::clone(&bytes_stream_to_udp)
        ) => {
            if let Err(e) = result {
                tracing::error!("[UDP] Stream → UDP error: {}", e);
                return Err(e);
            }
        }
        result = udp_to_stream(
            &stream,
            &udp_socket,
            is_connect,
            Arc::clone(&packets_udp_to_stream),
            Arc::clone(&bytes_udp_to_stream)
        ) => {
            if let Err(e) = result {
                tracing::error!("[UDP] UDP → Stream error: {}", e);
                return Err(e);
            }
        }
    }

    let packets_out = packets_stream_to_udp.load(Ordering::Relaxed);
    let bytes_out = bytes_stream_to_udp.load(Ordering::Relaxed);
    let packets_in = packets_udp_to_stream.load(Ordering::Relaxed);
    let bytes_in = bytes_udp_to_stream.load(Ordering::Relaxed);
    udp_span.record("packets_out", packets_out);
    udp_span.record("bytes_out", bytes_out);
    udp_span.record("packets_in", packets_in);
    udp_span.record("bytes_in", bytes_in);

    tracing::debug!(
        "[UDP] UDP over TCP proxy completed for stream {} (packets_out={}, packets_in={})",
        stream_id,
        packets_out,
        packets_in
    );
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
async fn read_initial_request(reader: &mut StreamReader) -> Result<(bool, SocketAddr)> {
    // Read isConnect (1 byte)
    let mut is_connect_buf = [0u8; 1];
    reader
        .read_exact(&mut is_connect_buf)
        .await
        .map_err(AnyTlsError::Io)?;

    let is_connect = is_connect_buf[0];

    if is_connect > 1 {
        return Err(AnyTlsError::Protocol(format!(
            "Invalid UDP over TCP isConnect value: {}",
            is_connect
        )));
    }

    tracing::debug!(is_connect, "[UDP] Parsed UDP over TCP format");

    // Read ATYP (1 byte)
    let mut atyp_buf = [0u8; 1];
    reader
        .read_exact(&mut atyp_buf)
        .await
        .map_err(AnyTlsError::Io)?;

    let atyp = atyp_buf[0];

    match atyp {
        0x01 => {
            // IPv4: 4 bytes IP + 2 bytes port
            let mut ip_buf = [0u8; 4];
            reader
                .read_exact(&mut ip_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let mut port_buf = [0u8; 2];
            reader
                .read_exact(&mut port_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let ip = std::net::Ipv4Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);

            Ok((is_connect == 1, SocketAddr::from((ip, port))))
        }
        0x04 => {
            // IPv6: 16 bytes IP + 2 bytes port
            let mut ip_buf = [0u8; 16];
            reader
                .read_exact(&mut ip_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let mut port_buf = [0u8; 2];
            reader
                .read_exact(&mut port_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let ip = std::net::Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes(port_buf);

            Ok((is_connect == 1, SocketAddr::from((ip, port))))
        }
        0x03 => {
            // Domain: length (1 byte) + domain + 2 bytes port
            let mut len_buf = [0u8; 1];
            reader
                .read_exact(&mut len_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let domain_len = len_buf[0] as usize;
            if domain_len == 0 || domain_len > 255 {
                return Err(AnyTlsError::Protocol("Invalid domain length".into()));
            }

            let mut domain_buf = vec![0u8; domain_len];
            reader
                .read_exact(&mut domain_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let domain = String::from_utf8(domain_buf)
                .map_err(|e| AnyTlsError::Protocol(format!("Invalid domain name: {}", e)))?;

            let mut port_buf = [0u8; 2];
            reader
                .read_exact(&mut port_buf)
                .await
                .map_err(AnyTlsError::Io)?;

            let port = u16::from_be_bytes(port_buf);

            // Resolve domain name with caching and timeout
            let addr = resolve_host_with_cache(&domain, port).await?;

            Ok((is_connect == 1, addr))
        }
        _ => Err(AnyTlsError::Protocol(format!(
            "Unknown address type: {}",
            atyp
        ))),
    }
}

/// Stream → UDP: Read packets from Stream, decode and send to UDP
///
/// Connect packets contain Length + Payload; non-Connect packets also carry a destination.
async fn stream_to_udp(
    stream: &Stream,
    udp: &UdpSocket,
    is_connect: bool,
    target_addr: &SocketAddr,
    packets_counter: Arc<AtomicU64>,
    bytes_counter: Arc<AtomicU64>,
) -> Result<()> {
    let stream_id = stream.id();
    let reader = stream.reader();
    let mut reader_guard = reader.lock().await;

    tracing::debug!("[UDP] Stream → UDP task started for stream {}", stream_id);

    loop {
        // Read one UDP packet (Length + Payload format)
        let (packet_target, payload) = match if is_connect {
            read_udp_packet(&mut reader_guard)
                .await
                .map(|payload| (*target_addr, payload))
        } else {
            read_udp_packet_non_connect(&mut reader_guard).await
        } {
            Ok(packet) => packet,
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
        let sent = udp.send_to(&payload, packet_target).await?;

        if sent != payload.len() {
            tracing::warn!("[UDP] Partial UDP send: {} / {} bytes", sent, payload.len());
        }
        packets_counter.fetch_add(1, Ordering::Relaxed);
        bytes_counter.fetch_add(sent as u64, Ordering::Relaxed);
    }

    Ok(())
}

/// UDP → Stream: Read from UDP, encode and send to Stream
///
/// Connect packets contain Length + Payload; non-Connect packets include the source address.
async fn udp_to_stream(
    stream: &Stream,
    udp: &UdpSocket,
    is_connect: bool,
    packets_counter: Arc<AtomicU64>,
    bytes_counter: Arc<AtomicU64>,
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

        // Encode according to the negotiated UoT v2 mode.
        let packet = if is_connect {
            encode_udp_packet_simple(&buf[..len])?
        } else {
            encode_udp_packet_non_connect(addr, &buf[..len])?
        };

        // Send to Stream using the send_data method
        if let Err(e) = stream.send_data(packet).await {
            tracing::error!("[UDP] Failed to send to stream: {}", e);
            return Err(AnyTlsError::Protocol("Channel send failed".into()));
        }
        packets_counter.fetch_add(1, Ordering::Relaxed);
        bytes_counter.fetch_add(len as u64, Ordering::Relaxed);
    }
}

/// Read one UDP packet from Stream
///
/// Format: | Length (2 bytes BE) | Payload |
/// Returns the payload (without length prefix)
async fn read_udp_packet(reader: &mut StreamReader) -> Result<Vec<u8>> {
    // Read 2-byte length (Big-Endian)
    let mut len_buf = [0u8; 2];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(AnyTlsError::Io)?;

    let len = u16::from_be_bytes(len_buf) as usize;

    if len == 0 {
        return Ok(Vec::new());
    }

    if len > MAX_UDP_PACKET_SIZE {
        return Err(AnyTlsError::Protocol(format!(
            "UDP packet too large: {} bytes",
            len
        )));
    }

    // Read the actual payload
    let mut data = vec![0u8; len];
    reader
        .read_exact(&mut data)
        .await
        .map_err(AnyTlsError::Io)?;

    Ok(data)
}

/// Read one non-connect UoT v2 packet: address, port, length, payload.
async fn read_udp_packet_non_connect(reader: &mut StreamReader) -> Result<(SocketAddr, Vec<u8>)> {
    let mut atyp_buf = [0u8; 1];
    reader
        .read_exact(&mut atyp_buf)
        .await
        .map_err(AnyTlsError::Io)?;
    let host = match atyp_buf[0] {
        0x00 => {
            let mut ip = [0u8; 4];
            reader.read_exact(&mut ip).await.map_err(AnyTlsError::Io)?;
            std::net::IpAddr::V4(ip.into()).to_string()
        }
        0x01 => {
            let mut ip = [0u8; 16];
            reader.read_exact(&mut ip).await.map_err(AnyTlsError::Io)?;
            std::net::IpAddr::V6(ip.into()).to_string()
        }
        0x02 => {
            let mut len = [0u8; 1];
            reader.read_exact(&mut len).await.map_err(AnyTlsError::Io)?;
            if len[0] == 0 {
                return Err(AnyTlsError::Protocol("Invalid domain length".into()));
            }
            let mut domain = vec![0; len[0] as usize];
            reader
                .read_exact(&mut domain)
                .await
                .map_err(AnyTlsError::Io)?;
            String::from_utf8(domain)
                .map_err(|e| AnyTlsError::Protocol(format!("Invalid domain name: {e}")))?
        }
        atyp => {
            return Err(AnyTlsError::Protocol(format!(
                "Unknown UoT address type: {atyp}"
            )));
        }
    };
    let mut port_buf = [0u8; 2];
    reader
        .read_exact(&mut port_buf)
        .await
        .map_err(AnyTlsError::Io)?;
    let port = u16::from_be_bytes(port_buf);
    let addr = resolve_host_with_cache(&host, port).await?;
    let payload = read_udp_packet(reader).await?;
    if payload.is_empty() {
        return Err(AnyTlsError::Protocol("Empty UDP packet".into()));
    }
    Ok((addr, payload))
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
        return Err(AnyTlsError::Protocol(format!(
            "UDP packet too large: {} bytes",
            payload.len()
        )));
    }

    // Write length (2 bytes, Big-Endian)
    buf.put_u16(payload.len() as u16);

    // Write payload
    buf.put_slice(payload);

    Ok(buf.freeze())
}

fn encode_udp_packet_non_connect(addr: SocketAddr, payload: &[u8]) -> Result<Bytes> {
    if payload.len() > MAX_UDP_PACKET_SIZE {
        return Err(AnyTlsError::Protocol(format!(
            "UDP packet too large: {} bytes",
            payload.len()
        )));
    }
    let mut buf = BytesMut::with_capacity(1 + 16 + 2 + 2 + payload.len());
    match addr {
        SocketAddr::V4(addr) => {
            buf.put_u8(0x00);
            buf.put_slice(&addr.ip().octets());
            buf.put_u16(addr.port());
        }
        SocketAddr::V6(addr) => {
            buf.put_u8(0x01);
            buf.put_slice(&addr.ip().octets());
            buf.put_u16(addr.port());
        }
    }
    buf.put_u16(payload.len() as u16);
    buf.put_slice(payload);
    Ok(buf.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    async fn reader_with_bytes(data: &[u8]) -> (mpsc::Sender<Bytes>, StreamReader) {
        let (tx, rx) = mpsc::channel(2);
        tx.send(Bytes::copy_from_slice(data)).await.unwrap();
        (tx, StreamReader::new(7, rx))
    }

    #[tokio::test]
    async fn test_initial_request_accepts_connect_and_non_connect() {
        for flag in [0, 1] {
            let mut request = vec![flag, 0x01, 127, 0, 0, 1];
            request.extend_from_slice(&5353u16.to_be_bytes());
            let (_tx, mut reader) = reader_with_bytes(&request).await;
            let (is_connect, target) = read_initial_request(&mut reader).await.unwrap();
            assert_eq!(is_connect, flag == 1);
            assert_eq!(target, "127.0.0.1:5353".parse().unwrap());
        }
    }

    #[tokio::test]
    async fn test_initial_request_rejects_unknown_flag() {
        let (_tx, mut reader) = reader_with_bytes(&[2, 1, 127, 0, 0, 1, 0, 53]).await;
        assert!(read_initial_request(&mut reader).await.is_err());
    }

    #[tokio::test]
    async fn test_non_connect_packet_decodes_address_and_payload() {
        // UoT address type 0 means IPv4; the datagram length covers the payload.
        let mut packet = vec![0x00, 127, 0, 0, 1];
        packet.extend_from_slice(&5353u16.to_be_bytes());
        packet.extend_from_slice(&3u16.to_be_bytes());
        packet.extend_from_slice(b"udp");
        let (_tx, mut reader) = reader_with_bytes(&packet).await;
        let (addr, payload) = read_udp_packet_non_connect(&mut reader).await.unwrap();
        assert_eq!(addr, "127.0.0.1:5353".parse().unwrap());
        assert_eq!(payload, b"udp");
    }

    #[test]
    fn test_non_connect_response_encodes_source_address() {
        let encoded =
            encode_udp_packet_non_connect("127.0.0.1:5353".parse().unwrap(), b"udp").unwrap();
        assert_eq!(&encoded[..7], &[0x00, 127, 0, 0, 1, 0x14, 0xe9]);
        assert_eq!(u16::from_be_bytes([encoded[7], encoded[8]]), 3);
        assert_eq!(&encoded[9..], b"udp");
    }

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
