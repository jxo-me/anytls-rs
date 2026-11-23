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
use crate::util::{AnyTlsError, Result, resolve_host_addresses_with_cache};
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
pub async fn handle_udp_over_tcp(stream: Arc<Stream>, legacy_v1: bool) -> Result<()> {
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
    let (is_connect, mut target_addresses) = if legacy_v1 {
        (
            false,
            vec!["0.0.0.0:0".parse().expect("valid unspecified address")],
        )
    } else {
        match read_initial_request(&mut reader_guard).await {
            Ok(addr) => addr,
            Err(e) => {
                tracing::error!("[UDP] Failed to read initial request: {}", e);
                return Err(e);
            }
        }
    };
    let target_addr = *target_addresses
        .first()
        .ok_or_else(|| AnyTlsError::Protocol("UDP target resolved to no addresses".into()))?;
    udp_span.record("target", field::display(target_addr));

    tracing::debug!("[UDP] Target UDP address: {}", target_addr);

    drop(reader_guard);

    // Step 2: Create UDP socket (bind to any available port)
    let udp_socket = bind_dual_stack_udp().await.map_err(|e| {
        tracing::error!("[UDP] Failed to create UDP socket: {}", e);
        AnyTlsError::Io(e)
    })?;

    if is_connect {
        target_addresses = connect_udp_to_any(&udp_socket, &target_addresses).await?;
    }

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
            &target_addresses,
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
async fn read_initial_request(reader: &mut StreamReader) -> Result<(bool, Vec<SocketAddr>)> {
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

            Ok((is_connect == 1, vec![SocketAddr::from((ip, port))]))
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

            Ok((is_connect == 1, vec![SocketAddr::from((ip, port))]))
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
            let addresses = resolve_host_addresses_with_cache(&domain, port).await?;

            Ok((is_connect == 1, addresses))
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
    target_addrs: &[SocketAddr],
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
                .map(|payload| (target_addrs.to_vec(), payload))
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

        tracing::trace!(
            "[UDP] Stream → UDP: {} bytes to {}",
            payload.len(),
            target_addrs
                .first()
                .map(ToString::to_string)
                .unwrap_or_else(|| "<none>".into())
        );

        // Send to UDP (target address already known from initial request)
        let sent = if is_connect {
            send_udp_connected_candidates(udp, &payload, &packet_target).await?
        } else {
            send_udp_to_candidates(udp, &payload, &packet_target).await?
        };

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

    let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE + 1];

    loop {
        // Receive from UDP
        let (len, addr) = match udp.recv_from(&mut buf).await {
            Ok((len, addr)) => (len, normalize_v4_mapped(addr)),
            Err(e) => {
                tracing::error!("[UDP] Failed to receive from UDP: {}", e);
                return Err(AnyTlsError::Io(e));
            }
        };
        if len > MAX_UDP_PACKET_SIZE {
            return Err(AnyTlsError::Protocol(
                "truncated or oversized UDP datagram".into(),
            ));
        }

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
async fn read_udp_packet_non_connect(
    reader: &mut StreamReader,
) -> Result<(Vec<SocketAddr>, Vec<u8>)> {
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
    let addresses = resolve_host_addresses_with_cache(&host, port).await?;
    let payload = read_udp_packet(reader).await?;
    Ok((addresses, payload))
}

/// Try candidate addresses only after an immediate local `send_to` error.
/// A successful datagram is never resent, preserving UDP's at-most-once behavior.
async fn send_udp_to_candidates(
    udp: &UdpSocket,
    payload: &[u8],
    candidates: &[SocketAddr],
) -> std::io::Result<usize> {
    let mut last_error = None;
    let local = udp.local_addr()?;
    for address in candidates {
        let address = map_v4_for_dual_stack(*address, local);
        match udp.send_to(payload, address).await {
            Ok(sent) => return Ok(sent),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no UDP destination addresses")
    }))
}

async fn connect_udp_to_any(
    udp: &UdpSocket,
    candidates: &[SocketAddr],
) -> std::io::Result<Vec<SocketAddr>> {
    let local = udp.local_addr()?;
    let mut last_error = None;
    for (index, address) in candidates.iter().enumerate() {
        match udp.connect(map_v4_for_dual_stack(*address, local)).await {
            Ok(()) => {
                return Ok(candidates[index..]
                    .iter()
                    .chain(&candidates[..index])
                    .copied()
                    .collect());
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no UDP destination addresses")
    }))
}

async fn send_udp_connected_candidates(
    udp: &UdpSocket,
    payload: &[u8],
    candidates: &[SocketAddr],
) -> std::io::Result<usize> {
    let local = udp.local_addr()?;
    let mut last_error = None;
    for address in candidates {
        if let Err(error) = udp.connect(map_v4_for_dual_stack(*address, local)).await {
            last_error = Some(error);
            continue;
        }
        match udp.send(payload).await {
            Ok(sent) => return Ok(sent),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no UDP destination addresses")
    }))
}

async fn bind_dual_stack_udp() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let ipv6 = (|| {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(false)?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&SockAddr::from("[::]:0".parse::<SocketAddr>().unwrap()))?;
        let socket: std::net::UdpSocket = socket.into();
        UdpSocket::from_std(socket)
    })();
    match ipv6 {
        Ok(socket) => Ok(socket),
        Err(ipv6_error) => UdpSocket::bind("0.0.0.0:0").await.map_err(|_| ipv6_error),
    }
}

fn map_v4_for_dual_stack(address: SocketAddr, local: SocketAddr) -> SocketAddr {
    match (address, local) {
        (SocketAddr::V4(v4), SocketAddr::V6(_)) => SocketAddr::V6(std::net::SocketAddrV6::new(
            v4.ip().to_ipv6_mapped(),
            v4.port(),
            0,
            0,
        )),
        _ => address,
    }
}

fn normalize_v4_mapped(address: SocketAddr) -> SocketAddr {
    match address {
        SocketAddr::V6(v6) => v6
            .ip()
            .to_ipv4_mapped()
            .map(|ip| SocketAddr::new(ip.into(), v6.port()))
            .unwrap_or(address),
        _ => address,
    }
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
            assert_eq!(target, vec!["127.0.0.1:5353".parse().unwrap()]);
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
        assert_eq!(addr, vec!["127.0.0.1:5353".parse().unwrap()]);
        assert_eq!(payload, b"udp");
    }

    #[tokio::test]
    async fn udp_send_tries_next_address_only_after_local_error() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let target = receiver.local_addr().unwrap();
        let sent =
            send_udp_to_candidates(&sender, b"packet", &["[::1]:9".parse().unwrap(), target])
                .await
                .unwrap();
        assert_eq!(sent, 6);
        let mut buf = [0; 16];
        let (len, _) = receiver.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..len], b"packet");
    }

    #[tokio::test]
    async fn udp_send_preserves_empty_datagrams() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let sent = send_udp_to_candidates(&sender, b"", &[receiver.local_addr().unwrap()])
            .await
            .unwrap();
        assert_eq!(sent, 0);
        let mut buf = [0; 1];
        let (len, _) = receiver.recv_from(&mut buf).await.unwrap();
        assert_eq!(len, 0);
    }

    #[tokio::test]
    async fn dual_stack_socket_sends_to_ipv4_candidates() {
        let relay = bind_dual_stack_udp().await.unwrap();
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        send_udp_to_candidates(&relay, b"v4", &[receiver.local_addr().unwrap()])
            .await
            .unwrap();
        let mut buf = [0; 8];
        let (len, _) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            receiver.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf[..len], b"v4");
    }

    #[tokio::test]
    async fn connected_udp_socket_filters_other_peers() {
        let expected = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let other = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = relay.local_addr().unwrap();
        connect_udp_to_any(&relay, &[expected.local_addr().unwrap()])
            .await
            .unwrap();
        expected.send_to(b"expected", relay_addr).await.unwrap();
        other.send_to(b"filtered", relay_addr).await.unwrap();

        let mut buf = [0; 32];
        let (len, _) =
            tokio::time::timeout(std::time::Duration::from_secs(1), relay.recv_from(&mut buf))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(&buf[..len], b"expected");
    }

    #[tokio::test]
    async fn connected_udp_tries_next_candidate_after_local_family_error() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let candidates = connect_udp_to_any(
            &relay,
            &["[::1]:9".parse().unwrap(), receiver.local_addr().unwrap()],
        )
        .await
        .unwrap();
        assert_eq!(candidates[0], receiver.local_addr().unwrap());
        send_udp_connected_candidates(&relay, b"fallback", &candidates)
            .await
            .unwrap();
        let mut buf = [0; 16];
        let (len, _) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            receiver.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf[..len], b"fallback");
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
