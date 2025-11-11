# Changelog

All notable changes to this project will be documented in this file. Dates use `YYYY-MM-DD`.

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

