# 测试与可观测性最小集（AnyTLS-RS）

## 1. 单元测试（Unit Tests）

- **帧编解码（Frame Codec）**
  - 覆盖 `protocol/frame.rs` 与 `protocol/codec.rs`
  - 场景：`Syn / Push / Fin / HeartRequest / HeartResponse` 等命令序列，校验 encode → decode 一致性
  - 可复制命令：
    ```bash
    cargo test frame --lib -- --exact
    cargo test codec --lib -- --nocapture
    ```

- **Padding 策略**
  - 目标：验证 `padding/factory.rs` 在不同填充策略下生成的随机前缀/后缀长度
  - 关注：自定义 padding 文件加载失败时的错误信息
  - 命令：
    ```bash
    cargo test padding --lib
    ```

- **错误映射（Error Mapping）**
  - 覆盖 `util/error.rs`、`util/tls.rs`、`client/session_pool.rs`
  - 重点：TLS 加载失败、密码校验失败、SYNACK 超时等场景正确映射为 `AnyTlsError`
  - 命令：
    ```bash
    cargo test error --lib -- --exact
    ```

## 2. 集成测试（Integration Tests）

- **本地 Server ↔ Client 回环（TCP + SOCKS5）**
  - `tests/tcp_roundtrip.rs`：启动本地 HTTP upstream，通过 AnyTLS + SOCKS5 完成 HTTP GET
  - 命令：
    ```bash
    cargo test --test tcp_roundtrip -- --nocapture
    ```

- **UDP-over-TCP 回环**
  - 使用新加的 `tests/udp_roundtrip.rs`
  - 过程：创建本地 UDP echo server → 通过 AnyTLS UDP Proxy 转发 → 校验回包
  - 命令：
    ```bash
    cargo test --test udp_roundtrip -- --nocapture
    ```

## 3. 基准测试（Benchmarks）

- **目标指标**
  - 会话复用并发：1 / 10 / 100 个并发流，关注 p50 / p95 延迟与吞吐（MB/s）
  - Session 预热影响：`min_idle_session` 在 1/5/10 情况下的重连延迟

- **工具建议**
  - 使用 Criterion：`benches/e2e_bench.rs` 已提供骨架
  - 增补 `bench_session_pool_latency` 与 `bench_udp_over_tcp_roundtrip`
  - 示例命令：
    ```bash
    cargo bench --bench e2e_bench
    cargo bench --bench session_bench -- --sample-size 50
    ```

- **产出要求**
  - 保存 `target/criterion/**/report/index.html`
  - 记录 CSV：`criterion/export/{metric}_summary.csv`
  - 构建基准矩阵：列出并发数、p50/p95 延迟、吞吐、CPU 使用

## 4. 可观测性（Observability）

- **Tracing 篇**
  - **握手阶段**（server `handle_connection`、client `Client::connect`）
    - span：`handshake`
    - fields：`session_id`（server 侧）、`peer_addr`、`tls_version`、`cipher_suite`
    - 错误字段：`error.cause`, `error.chain`

- **会话复用 / Frame 处理**（`session::recv_loop`、`Session::process_stream_data`、`Stream::write_frame`）
  - span：`anytls.session.recv`、`anytls.session.process_stream_data`、`frame_process`
  - fields：`session_id`, `role`, `frame.command`, `stream_id`, `payload_len`, `bytes_in`, `bytes_out`, `iterations`

  - **心跳**（`command::HeartRequest` / `HeartResponse`）
    - span：`heartbeat`
    - fields：`session_id`, `peer_version`, `status`（success/timeout/retry）, `latency_ms`

- **FIN / 超时回收**（`Stream::close`、`Session::close_idle`、`SessionPool::cleanup_expired`）
  - span：`stream_close`, `session_timeout`, `anytls.session_pool.cleanup`
  - fields：`session_id`, `stream_id`, `bytes_sent`, `bytes_received`, `idle_duration`, `removed`, `remaining`, `cause`（manual/timeout/error）

- **UDP-over-TCP 转发**
  - span：`anytls.udp.proxy`
  - fields：`stream_id`, `local_udp`, `target`, `packets_in/out`, `bytes_in/out`

- **日志建议**
  - 默认级别：`RUST_LOG=info,anytls=debug`
  - 将 `session_id` / `stream_id` 作为全局字段挂到 `info_span!`
  - 在关键路径对齐 sing-box 字段（如 `idle_session_timeout`）方便对比

- **Metrics（可选）**
  - 定义简单计数器：`sessions_open`、`streams_active`、`udp_packets_forwarded`
  - 替代方案：先以结构化日志输出，后续再接入 Prometheus/OpenTelemetry

---

## 附录：推荐命令速查

```bash
# 单测
cargo test frame --lib -- --exact
cargo test padding --lib
cargo test error --lib -- --exact

# 集成测试
cargo test --tests tcp_roundtrip
cargo test --test socks5_connectivity
cargo test --test udp_roundtrip -- --nocapture

# 基准
cargo bench --bench e2e_bench
cargo bench --bench session_bench -- --sample-size 50

# 运行带埋点的 server/client
RUST_LOG=info,anytls=debug cargo run --bin anytls-server ...
RUST_LOG=info,anytls=debug cargo run --bin anytls-client ...
```

