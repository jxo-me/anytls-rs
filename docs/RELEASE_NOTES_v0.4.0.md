# AnyTLS-RS v0.4.0 发布说明

发布日期：2025-11-08  
仓库标签：`v0.4.0`

## 亮点

- **HTTP 代理**：`anytls-client` 新增 `-H/--http-listen` 参数，支持 HTTP CONNECT / 明文代理，便于与浏览器、本地工具集成。
- **会话池参数短选项**：客户端与服务端补全 `-I/-T/-M`，与 sing-box 配置字段一一对应，开发脚本也已同步。
- **UDP-over-TCP 对齐**：服务端在检测到 sing-box v1.2+ UDP 请求时主动发送 SYNACK，客户端修复 last peer 追踪，集成测试覆盖回环。
- **端到端验证脚本**：新增 `scripts/dev-verify.sh`（包含 SOCKS5/HTTP 验证）与 `tests/tcp_roundtrip.rs`；`docs/03-test-and-observability.md` 描述最小测试矩阵。
- **可观测性增强**：在握手、会话循环、流关闭路径补充 `tracing` 字段（`session_id`、`stream_id`、`bytes_in/out`、TLS 信息）。

## 兼容性与升级提示

- CLI 参数：
  - 新增/调整：`anytls-client --http-listen` 现有短参 `-H`；`anytls-{client,server}` 支持 `-I/-T/-M`。
  - 原有长参数保持兼容，脚本 `scripts/dev-up.sh` 支持环境变量映射。
- 文档定位：
  - 快速上手：`docs/01-dev-quickstart.md`
  - sing-box 集成计划：`docs/02-feature-mvp-plan.md`
  - 测试与观测：`docs/03-test-and-observability.md`

## 测试与验证

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo bench --bench e2e_bench`（可选，用于获取 p50/p95 指标）
- `cargo publish --dry-run`

## 后续规划

- 主动心跳/空闲检测增强与最小观测指标对齐
- 更多 padding 策略与配置化选项
- 自动化 e2e 脚本支持 sing-box 多版本验证
- 发布流程文档化与自动化（CI 签发包、CHANGELOG 自动生成）

---

欢迎在 [GitHub Issues](https://github.com/jxo-me/anytls-rs/issues) 反馈问题或提交 PR。🏷️记得升级至 `anytls-rs = "0.4.0"`。

