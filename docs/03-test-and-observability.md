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

- **基础代理连通性（SOCKS5）**
  - `tests/basic_proxy.rs`：使用随机端口与内建 TCP echo server，同步验证 server ↔ client ↔ upstream 全链路
  - 命令：
    ```bash
    cargo test --test basic_proxy -- --nocapture
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

- **2025-11-08 基准记录（cargo bench --bench e2e_bench）**

  | 场景 | p50 time | 备注 |
  |------|----------|------|
  | `e2e_stream_open_and_send/64` | 8.08 µs | 单条流建立 + 64B |
  | `e2e_stream_open_and_send/16384` | 10.04 µs | 单条流建立 + 16KB |
  | `e2e_multiple_streams/concurrent_streams/1` | 8.18 µs | 单条流复用 |
  | `e2e_multiple_streams/concurrent_streams/10` | 34.51 µs | 10 并发流 |
  | `e2e_multiple_streams/concurrent_streams/100` | 207.01 µs | 100 并发流 |
  | `e2e_data_throughput/throughput/64` | 76.71 µs | 对应 ~0.83 MB/s |
  | `e2e_data_throughput/throughput/4096` | 12.59 µs | 对应 ~325 MB/s |
  | `e2e_session_startup_and_streams` | 11.75 µs | 会话启动 + 5 条流 |
  | `udp_over_tcp_roundtrip/64` | 309.86 ms | 本地 UDP 回环，受 sleep/网络模拟影响 |

  > 所有测试在 macOS + Rust 1.78 工具链上运行，criterion 默认 100 samples。`udp_over_tcp_roundtrip` 需手动调整采样时间（警告提示），目前基准值约 0.31s。未来可考虑缩减样本或优化测试逻辑。

- **会话内部基准（cargo bench --bench session_bench）**

  | 场景 | p50 time | 备注 |
  |------|----------|------|
  | `frame_encoding/encode/64` | 30.36 ns | Frame 编码基线 |
  | `frame_decode/decode/64` | 1.07 µs | 解码 64B 帧 |
  | `stream_creation` | 5.27 µs | 新建流（Mock） |
  | `session_startup_complete` | 16.84 µs | 会话启动 + 背景任务 |
  | `session_write_frame/write_frame/64` | 3.80 µs | 包含 write + flush |
  | `session_control_frames/heart_request` | 3.99 µs | 心跳请求写入 |
  | `session_multiple_streams/open_streams/20` | 50.07 µs | 连续开启 20 条流 |
  | `padding_factory_generate_sizes` | 3.66 µs | padding 策略计算 |

- **流级操作（cargo bench --bench stream_bench）**

  | 场景 | p50 time | 备注 |
  |------|----------|------|
  | `stream_write/write/64` | 5.13 µs | `Stream::send_data` |
  | `stream_write/write/4096` | 5.25 µs | |
  | `stream_read/read/64` | 582 ns | `Stream::poll_read` 模拟 |
  | `streamreader_read/read/4096` | 666 ns | `StreamReader` 独立读 |
  | `stream_concurrent_read_write` | 52.80 µs | 读写并发压力 |

- **会话池表现（cargo bench --bench session_pool_bench）**

  | 场景 | p50 time | 备注 |
  |------|----------|------|
  | `session_pool_add_and_get/1` | 8.04 µs | 添加 + 取回 |
  | `session_pool_add_and_get/50` | 161.85 µs | 批量 50 条 |
  | `session_pool_concurrent_get/50pool_20gets` | 167.69 µs | 20 并发获取 |
  | `session_pool_cleanup` | 151.84 ms | 100 sample；可调低 sample 或优化逻辑 |

- **通用对比基准（cargo bench --bench comparison_bench）**

  | 场景 | p50 time | 备注 |
  |------|----------|------|
  | `frame_encoding_strategies/encode_each_time/1024` | 96.42 ns | 实时编码 |
  | `frame_encoding_strategies/pre_encoded_clone/1024` | 13.00 ns | 预编码复用 |
  | `stream_creation_overhead` | 5.16 µs | |
  | `session_startup_overhead` | 15.06 µs | |
  | `data_frame_throughput/1024B_x1000` | 298.50 µs | ≈3.4 GB/s |
  | `critical_path_operations/frame_encode_critical` | 105.44 ns | |
  | `critical_path_operations/stream_send_critical` | 5.90 µs | 单次 send |

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
  - 说明：自 0.4.1 起 `Session::close` 会广播 `close_notify` 并为 writer shutdown 设置 1s 超时，相关日志降级为 `debug`，方便排查但不会干扰测试

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

