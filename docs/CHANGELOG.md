# Changelog

All notable changes to this project will be documented in this file. Dates use `YYYY-MM-DD`.

## [0.5.7] - 2026-11-24

### Fixed
- UoT now supports legacy v1 and v2 connected/unconnected modes, including empty datagrams, peer filtering, dual-stack addressing, and truncated packet detection.
- TCP streams now preserve queued data across peer FIN, propagate EOF and errors to waiters, fragment large writes into valid protocol frames, and supervise forwarding tasks.
- Session and stream handling now validates Settings and stream IDs, bounds control writes, and performs reliable cancellation and close cleanup.
- DNS and TCP proxy paths now try multiple resolved addresses and avoid retrying successfully transmitted UDP datagrams.
- Certificate renewals are rejected when not yet valid or expired, while certificate validity is recalculated independently of file watching.
- Oversized or malformed control frames are rejected, and server connection/transport tasks have bounded concurrency and cleanup.

## [0.5.6] - 2026-11-24

### Fixed
- DNS caching now retains multiple IP addresses per hostname, applies the requested port per connection, and rotates candidates; TCP connections can fall back to another resolved address.
- Certificate validity summaries now use the current time and report accurate days remaining.
- Protocol frame encoding rejects payloads larger than the wire format can represent.
- Server handshakes now have concurrency and timeout limits, and transient accept errors use exponential backoff.

### Security
- Bounded unauthenticated handshakes to reduce resource exhaustion from slow or excessive client connections.
- Upgraded DNS resolution to `idna` 1.x, fixing RUSTSEC-2024-0421.
- Updated Hickory DNS and `time` to versions that include fixes for the advisories reported by `cargo audit`.

### Changed
- Replaced the unmaintained `trust-dns` resolver with maintained `hickory-resolver` and moved PEM parsing to `rustls-pki-types`.
- Set the declared minimum Rust version to 1.88 to match the selected Hickory DNS release.
- Removed the unused direct `serde` dependency.
- Updated GitHub Actions to versions using the Node.js 24 runtime and documented reviewed fork updates.

## [0.5.5] - 2026-11-23

### Security
- Replaced `chrono` with `jiff` in certificate validity formatting to remove the vulnerable `chrono` dependency (RUSTSEC-2026-0543).

## [0.5.4] - 2025-11-11

### Added
- **Network Stability Toolkit**
  - Introduced `util::net::configure_tcp_stream` enabling `TCP_NODELAY` and keepalive hints for all client/server sockets
  - Added `util::dns_cache::resolve_host_with_cache` to provide timed DNS caching with round-robin address selection

### Changed
- Client and server now automatically apply low-latency socket options after each successful `TcpStream::connect`
- TCP/UDP proxy paths reuse the shared DNS cache, reducing repeated lookups and latency spikes on popular domains
- Updated benchmarking/test helpers to accommodate new utilities

### Fixed
- Eliminated repeated DNS timeouts on high-traffic domains by combining caching with connection timeout handling
- Resolved Clippy warnings by tightening stream configuration and DNS cache control flow


## [0.5.3] - 2025-11-11

### Added
- **Connection Timeout Protection**: Critical fix for slow/unreachable websites
  - TCP connection timeout: 15 seconds (includes DNS resolution + TCP handshake)
  - DNS resolution timeout: 10 seconds for UDP proxy
  - Prevents indefinite hanging on DNS poisoning or unreachable targets
  - Timeout errors are properly reported to clients via SYNACK frames

### Fixed
- **Major**: Server-side proxy connections no longer hang indefinitely
  - Fixes YouTube, Google and other websites not loading or loading very slowly
  - DNS resolution failures now fail fast (10s) instead of hanging forever
  - Connection attempts to unreachable hosts timeout after 15s
  - Resources are properly released after timeout
- Improved error messages for timeout scenarios

### Performance
- Significantly improved responsiveness when accessing slow or unreachable websites
- Reduced resource consumption by releasing blocked connections faster
- Better concurrent connection handling due to timeout-based resource cleanup

### Documentation
- Added `docs/PROXY_TIMEOUT_OPTIMIZATION.md` with detailed problem analysis and solutions

## [0.5.2] - 2025-11-11

### Changed
- **Logging Refinement**: Further optimized log levels for production use
  - FrameCodec logs downgraded from `info` to `debug` (high-frequency protocol operations)
  - Server-side proxy operations downgraded to `debug`/`trace` levels
  - Client-side operations (SOCKS5, HTTP, UDP, session pool) downgraded to `debug`
  - Removed all emoji from log messages for cleaner output
  - Info level now strictly reserved for connection-level events only:
    - Server: "Listening on", "New connection from", "Session created"
    - Client: Proxy listeners "Listening on"
    - Certificate reload events

### Performance
- Reduced log volume in production environments by 70-80%
- Debug and trace logs provide detailed troubleshooting when needed

## [0.5.1] - 2025-11-11

### Fixed
- Windows build compatibility: Added conditional compilation for Unix-only signal handling
  - `tokio::signal::unix` imports now restricted to Unix platforms with `#[cfg(unix)]`
  - SIGHUP signal handler only compiled on Unix-like systems (Linux, macOS)
  - Windows users are notified to use `--watch-cert` for automatic certificate reload
  - Help text for SIGHUP signal now only displayed on Unix platforms

## [0.5.0] - 2025-11-11

### Added
- **TLS Certificate Hot Reload**: Automatic and manual certificate reloading without downtime
  - File watching with `--watch-cert` flag for automatic reload on certificate changes
  - Manual reload via SIGHUP signal (`kill -HUP <pid>`)
  - Certificate information display with `--show-cert-info` flag
  - Certificate expiry monitoring and warnings with configurable threshold
  - Atomic TLS acceptor updates ensuring zero-downtime certificate rotation
- Certificate analysis utilities (`CertificateInfo`, `CertReloader`)
  - Detailed certificate information extraction (subject, issuer, validity, SANs)
  - Certificate status detection (valid, expiring, expired)
  - Self-signed certificate identification
- Comprehensive integration tests for certificate reload functionality

### Changed
- **Logging Optimization**: Refined log levels for better production use
  - Downgraded high-frequency operations from `info` to `debug`/`trace`
  - Retained important events (connections, sessions) at `info` level
  - Added `-L/--log-level` flag to server and client for runtime control
- **Tokio Dependencies**: Optimized from `full` feature to specific features
  - Reduced to: `macros`, `rt-multi-thread`, `io-util`, `io-std`, `net`, `sync`, `time`, `signal`, `fs`
  - Smaller binary size and faster compilation times
  - Maintained `full` feature as an optional fallback
- Server TLS acceptor architecture refactored to support hot-reloading via `Arc<RwLock<Arc<TlsAcceptor>>>`

### Dependencies
- Added `notify = "8.2"` for file system monitoring
- Added `x509-parser = "0.18"` for certificate parsing
- Added `chrono = "0.4"` for date/time handling
- Added `tempfile = "3.8"` and `base64 = "0.22"` for testing

### Documentation
- Added comprehensive TLS certificate reload guide (`docs/TLS_CERT_RELOAD_GUIDE.md`)
- Updated help text with certificate options and signal handling documentation

### Testing
- Added 5 integration tests for certificate loading and reload functionality
- All 73 tests passing with full coverage of new features

## [0.4.1] - 2025-11-09

### Fixed
- Session shutdown now notifies background tasks and times out lingering I/O,避免 `recv_loop`/`process_stream_data` 阻塞导致测试挂起。
- 服务器端遇到 `SessionClosed` 时降级为调试日志，减少重连流程中的误报警。

### Testing
- `tests/basic_proxy.rs` 改为使用动态端口与内建 echo server，彻底剥离对外部网络的依赖。
- `tests/server_restart.rs`、`tests/basic_proxy.rs` 的清理流程增强，所有后台任务在测试结尾都会显式关闭。

## [0.4.0] - 2025-11-08

### Added
- HTTP proxy support via `anytls-client -H/--http-listen`.
- Short CLI flags `-I/-T/-M` for session pool tuning on both client and server.
- Active heartbeat monitoring with automatic session recovery and enhanced tracing spans.
- Comprehensive performance baseline (`cargo bench`) documentation and release notes.

### Fixed
- Session pool no longer retains closed sessions after disconnections.
- Heartbeat failures immediately close stale connections and unblock SYNACK waiters.

### Packaging
- Release archives now include binaries, `LICENSE`, `README` files and changelog documentation.

## [0.3.0] - 2025-11-03

### Added
- Baseline AnyTLS client/server implementation with SOCKS5 proxy support.
- UDP-over-TCP support aligned with sing-box outbound behaviour.
- Session pool reuse and padding factory configuration.
- Initial automation scripts (`scripts/dev-up.sh`, `scripts/dev-verify.sh`) and troubleshooting docs.

### Fixed
- Stream reader refactor eliminating deadlocks and improving throughput.

---

For older history, refer to version tags in the Git repository.

