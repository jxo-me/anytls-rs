# Changelog

All notable changes to this project will be documented in this file. Dates use `YYYY-MM-DD`.

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

