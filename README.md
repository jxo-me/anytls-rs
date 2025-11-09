# AnyTLS-RS

[![Version](https://img.shields.io/badge/version-0.4.0-blue.svg)](https://github.com/jxo-me/anytls-rs)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-blue.svg)](https://doc.rust-lang.org/edition-guide/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

高性能、可观测的 AnyTLS 协议 Rust 实现，专注缓解 TLS-in-TLS 指纹识别问题，支持 sing-box outbound ⇄ anytls-rs 服务端的端到端集成。

[English Version](README.en.md)

---

## ✨ 核心特性

- **多协议代理**：内置 SOCKS5 代理，新增 HTTP CONNECT/明文代理 (`anytls-client -H/--http-listen`)
- **会话复用**：session pool 支持自定义空闲检查/超时/预热（`-I/-T/-M` 与环境变量映射）
- **UDP-over-TCP**：兼容 sing-box v1.2 行为，自动发送 SYNACK，支持回环集成测试
- **TLS 管理**：可加载 PEM 证书，也可自动生成 `anytls.local` 自签证书（脚本自动完成）
- **脚本与自动化**：`scripts/dev-up.sh` 与 `scripts/dev-verify.sh` 提供最短启动与校验
- **文档完备**：快速画像、开发者上手、MVP 方案、FAQ、ADR 全量覆盖
- **观测增强**：结构化日志（`tracing`）、session id/stream id/span 埋点规划

---

## 🚀 快速上手

### 1. 环境要求

- Rust 1.70+ / cargo（推荐使用 rustup）
- 可选：`openssl`（如需导入现有证书）
- macOS/Linux 需允许脚本执行权限：`chmod +x scripts/*.sh`

### 2. 最短体验脚本

```bash
# 启动服务端 + 客户端（SOCKS5 监听 127.0.0.1:1080）
./scripts/dev-up.sh

# 校验 HTTP 与 SOCKS5 代理是否可用，完成后自动清理
./scripts/dev-verify.sh
```

脚本默认使用 `examples/singbox/certs/anytls.local.{crt,key}`。如果端口冲突，可通过 `SERVER_ADDR`、`CLIENT_ADDR`、`HTTP_ADDR` 环境变量覆盖。

### 3. 手动运行（两个终端）

```bash
# 终端 A：anytls-server（自签或手动提供证书）
cargo run --release --bin anytls-server -- \
  -l 0.0.0.0:8443 \
  -p your_password \
  --cert ./examples/singbox/certs/anytls.local.crt \
  --key  ./examples/singbox/certs/anytls.local.key \
  -I 30 -T 120 -M 1

# 终端 B：anytls-client（SOCKS5 + HTTP 代理）
cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -s 127.0.0.1:8443 \
  -p your_password \
  -I 30 -T 120 -M 1 \
  -H 127.0.0.1:8080

# 第三终端：验证
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
curl -x http://127.0.0.1:8080 http://httpbin.org/get
```

---

## 🧩 sing-box 集成

- 示例配置：`examples/singbox/outbound-anytls.jsonc`
- 快速指引：`examples/singbox/README.md`
- 验证配置：`sing-box check -c examples/singbox/outbound-anytls.jsonc`
- 关键字段映射：

| sing-box 字段 | anytls-rs CLI/脚本 | 说明 |
| --- | --- | --- |
| `password` | `anytls-{server,client} -p` | 必须一致 |
| `idle_session_check_interval` | `-I / IDLE_SESSION_CHECK_INTERVAL` | 秒 |
| `idle_session_timeout` | `-T / IDLE_SESSION_TIMEOUT` | 秒 |
| `min_idle_session` | `-M / MIN_IDLE_SESSION` | 预热会话数 |
| `tls.certificate_path` | `--cert` / `CERT_PATH` | 支持自签证书 |

---

## 🗺️ 项目结构

```
anytls-rs/
├── docs/                       # 文档（画像/上手/FAQ/ADR 等）
├── examples/singbox/           # sing-box outbound 示例
├── scripts/                    # 本地启动与验证脚本
├── src/
│   ├── bin/                    # CLI 入口（anytls-server/client）
│   ├── client/                 # 客户端核心（SOCKS5/HTTP/Session Pool/UDP-over-TCP）
│   ├── server/                 # 服务端核心（TCP/UDP 处理器）
│   ├── protocol/               # 帧协议定义与编解码
│   ├── session/                # 会话与流复用实现
│   └── util/                   # TLS、认证、错误等基础设施
├── tests/                      # 集成测试（含 UDP 回环）
└── benches/                    # 性能基准
```

详细画像请查看 `docs/00-project-radar.md`。

---

## ⚙️ CLI 快速参考

### anytls-server

| 选项 | 说明 |
| --- | --- |
| `-l, --listen <ADDR>` | 监听地址（默认 `0.0.0.0:8443`） |
| `-p, --password <PASSWORD>` | 共享密码（必填） |
| `--cert <FILE>` / `--key <FILE>` | PEM 证书与私钥（可选） |
| `-I, --idle-session-check-interval <SECS>` | 推荐给客户端的检查间隔 |
| `-T, --idle-session-timeout <SECS>` | 推荐空闲超时 |
| `-M, --min-idle-session <COUNT>` | 推荐保持的空闲会话数 |
| `-V, --version` | 显示版本信息 |

### anytls-client

| 选项 | 说明 |
| --- | --- |
| `-l, --listen <ADDR>` | SOCKS5 监听地址（默认 `127.0.0.1:1080`） |
| `-s, --server <ADDR>` | 服务端地址（默认 `127.0.0.1:8443`） |
| `-p, --password <PASSWORD>` | 共享密码（必填） |
| `-H, --http-listen <ADDR>` | HTTP 代理监听地址（可选） |
| `-I, --idle-session-check-interval <SECS>` | 会话检查间隔（默认 30） |
| `-T, --idle-session-timeout <SECS>` | 会话空闲超时（默认 60） |
| `-M, --min-idle-session <COUNT>` | 预热空闲会话数（默认 1） |
| `-V, --version` | 显示版本信息 |

环境变量版本可在 `docs/01-dev-quickstart.md` 与 `scripts/dev-up.sh` 中查阅。

---

## ✅ 测试与基准

- 单测：帧编解码、padding、错误映射等
- 集成测试：`tests/udp_roundtrip.rs`（UDP-over-TCP 回环）、SOCKS5/HTTP 代理本地验证
- 基准：`cargo bench`，包含会话并发、吞吐、UDP-over-TCP 延迟
- 自动化：`./scripts/dev-verify.sh` 会执行最短验证流程，便于回归

观测与测试最小集请参考 `docs/03-test-and-observability.md`。

---

## 📚 推荐阅读

- `docs/00-project-radar.md` —— 项目快速画像与风险盘点
- `docs/01-dev-quickstart.md` —— 开发者快速上手（命令集合 + 踩坑）
- `docs/02-feature-mvp-plan.md` —— sing-box MVP 增量方案
- `docs/adr/0001-singbox-anytls-e2e.md` —— 端到端架构决策记录
- `docs/FAQ.md` —— 常见问题与参数对照
- `docs/TROUBLESHOOTING.md` —— 故障排除手册

---

## 🛠️ 开发与贡献

```bash
# 检查
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 运行测试
cargo test

# 运行基准
cargo bench
```

欢迎通过 Issues / PR 提交建议，提交前请确保通过格式化、Clippy、测试，并更新相关文档。

---

## 🔐 安全与隐私

- TLS：基于 `rustls`，默认启用 TLS1.2/1.3，可自签或使用外部证书
- 认证：SHA256 + padding 策略，支持自定义 padding 文件
- 会话：支持最小空闲连接保留，降低重建成本
- 观测：`RUST_LOG=info,anytls=debug` 可获得丰富 tracing 日志，可根据文档扩展 span

---

## 📦 许可

本项目使用 MIT License，详情见 [LICENSE](LICENSE)。

---

## 🙏 致谢

- [anytls-go](https://github.com/anytls/anytls-go) —— 协议参考实现
- [sing-box](https://github.com/SagerNet/sing-box) —— outbound 配置与互通参考
- 所有贡献者与社区伙伴

---

**如果这个项目对你有帮助，欢迎 Star ⭐ 支持！**
