# 项目快速画像（AnyTLS-RS）

## 1. 代码地图（按职责）

- 顶层结构  
  - `.github/workflows/` — GitHub Actions：多平台构建、测试、基准与发布流水线  
  - `benches/` — 会话、流、TLS、客户端-服务端等性能基准  
  - `docs/` — 架构、测试、指南、版本总结与发布文档  
  - `scripts/` — 构建/发布辅助脚本（如 `release.sh`）  
  - `sing-box/` — 上游 sing-box Go 实现全集，用于协议对齐与行为验证  
  - `src/` — 核心库与可执行程序  
  - `tests/` — 集成测试：SOCKS5、心跳、并发、SYNACK 超时等  
  - `Cargo.toml` / `Cargo.lock` — 包元数据与依赖锁定

- `src/` 模块职责  
  - `protocol/frame.rs`、`protocol/codec.rs` — AnyTLS 帧定义、命令集、编码/解码器（基于 `tokio_util::codec`）  
  - `session/session.rs`、`session/stream.rs`、`session/stream_reader.rs` — 会话复用、逻辑流、独立读取器（v0.2 消除锁竞争）  
  - `padding/factory.rs` — Padding 策略工厂：动态长度、MD5 校验等混淆手段  
  - `util/error.rs`、`util/auth.rs`、`util/tls.rs`、`util/string_map.rs` — 错误类型、SHA256+salt 认证、rustls 配置、轻量映射结构  
  - `client/client.rs`、`client/socks5.rs`、`client/session_pool.rs`、`client/udp_client.rs` — 客户端握手/复用、SOCKS5 接入、会话池扩缩容（v0.3）、UDP-over-TCP  
  - `server/server.rs`、`server/handler.rs`、`server/udp_proxy.rs` — 服务端监听、流量转发、UDP 代理  
  - `bin/client.rs`、`bin/server.rs` — CLI 解析（`clap`）与启动逻辑

- 核心热路径  
  `client/socks5.rs` → `client/session_pool.rs` → `session::{Session, Stream}` → `protocol::{Frame, FrameCodec}` → `util/tls.rs`（tokio 异步 + rustls TLS I/O）

- 主要外部依赖  
  `tokio`、`tokio-util`、`rustls`、`tokio-rustls`、`rcgen`、`bytes`、`sha2`、`md5`、`rand`、`tracing`、`tracing-subscriber`、`serde`、`thiserror`、`anyhow`

## 2. 技术栈与依赖

| 依赖 | 版本 | 作用 | 备选/备注 |
|------|------|------|-----------|
| `tokio` | 1.48.0（full） | 异步运行时、网络 I/O | `async-std`（替换成本高） |
| `rustls` | 0.23.x | 纯 Rust TLS | `boring-tls`（需 C） |
| `tokio-rustls` | 0.26.x | rustls + tokio 适配 | — |
| `rcgen` | 0.14.x | 自签证书生成 | `openssl` CLI |
| `bytes` | 1.10.x | 字节缓冲 | `smallvec` |
| `tokio-util` | 0.7.x | Codec/Stream 工具 | `futures-codec` |
| `serde` | 1.x | 配置序列化 | — |
| `sha2` | 0.10.x | SHA256 密码散列 | `blake3` |
| `md5` | 0.8.x | Padding 校验 | 可替换或移除 |
| `rand` | 0.9.x | Padding 随机数 | — |
| `tracing` | 0.1.x | 结构化日志 | `log` + `env_logger` |
| `tracing-subscriber` | 0.3.x | 日志订阅/过滤 | — |
| `thiserror` | 2.0 | 错误类型定义 | `eyre` |
| `anyhow` | 1.0 | 错误包装 | `color-eyre` |
| `tokio-test` | 0.4（dev） | 异步测试辅助 | — |
| `criterion` | 0.7（dev） | 性能基准（HTML 报告） | — |

Rust Edition：2024；工具链需 Rust ≥1.70。

## 3. 运行面（最小可运行）

- 可执行程序：`anytls-server`、`anytls-client`（内置 SOCKS5）
- README 最短启动命令：

  ```bash
  cargo run --release --bin anytls-server -- \
    -l 0.0.0.0:8443 \
    -p your_password

  cargo run --release --bin anytls-client -- \
    -l 127.0.0.1:1080 \
    -s server.example.com:8443 \
    -p your_password

  curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
  ```

- 常用 CLI 参数  
  - 服务端：`--listen/-l <ADDR>`、`--password/-p <PASSWORD>`、`--cert <FILE>`、`--key <FILE>`  
  - 客户端：`--listen/-l <ADDR>`、`--server/-s <ADDR>`、`--password/-p <PASSWORD>`

## 4. 风险与技术债盘点

| 项目 | 影响面 | 紧急度 | 建议 |
|------|--------|--------|------|
| 会话复用/流控参数（`min_idle_session`, `idle_session_*`, 心跳、SYNACK 超时）与 sing-box outbound 不完全对齐 | 高并发或长连接场景下会话耗尽、频繁重建 | High | 立即：提供 CLI/脚本参数对齐（已支持 `--idle-session-*`），补基准 & e2e |
| UDP-over-TCP 行为与 sing-box v1.12.12 协议差异 | sing-box outbound 集成失败、UDP 延迟/丢包 | High | 立即：复用 `sing-box/protocol/anytls` 用例交叉测试 |
| 认证散列与 padding（SHA256+MD5）策略 | 安全性和观测性；MD5 带来质疑 | Medium | 跟进：提供可配置替代，记录安全评估 |
| TLS 证书管理（自签/LE）流程未固化 | 生产部署易误配或过期，影响上线体验 | Medium | 跟进：脚本化证书生成/轮换，补文档 |
| 与 sing-box 参考实现行为一致性（握手字段、心跳响应） | 互通回归风险；排障困难 | High | 立即：维护字段对照表与回归测试 |
| `tracing` 埋点覆盖不足（会话、心跳、异常路径） | 线上问题排查困难 | Medium | 跟进：补关键 span/fields，接入日志聚合 |
| `sing-box/` 目录版本滞后 | 参考实现漂移导致行为偏差 | Low | 观察：定期同步上游 commit，标记差异 |

