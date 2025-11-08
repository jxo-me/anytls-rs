# sing-box outbound ⇄ anytls-rs 服务端示例

## 1. 场景说明

- sing-box outbound 作为客户端，通过 AnyTLS 协议连接部署在内网的 `anytls-rs` 服务端。
- 目标：最小化配置即可验证 TCP（SOCKS5 → HTTP）与 UDP-over-TCP（DNS）链路，复用连接并隐藏 TLS 指纹。

## 2. 文件说明

| 文件 | 作用 |
|------|------|
| `outbound-anytls.jsonc` | sing-box outbound 配置模板（可直接校验） |
| `run-singbox.sh` | 一键启动 sing-box 的示例脚本（可选） |
| `README.md` | 使用说明 |

## 3. 先决条件

- Rust ≥ 1.70，用于运行 `anytls-rs`。
- 已编译 `sing-box` 可执行文件（建议 v1.12.12+）。
- `curl`、`dig` 等测试工具。

## 4. 步骤

1. **启动 anytls-rs 服务端**

   ```bash
   PASSWORD=your_password
   LISTEN_ADDR="0.0.0.0:8443"
   CERT_DIR="/Users/mickey/dev/rust/anytls-rs/examples/singbox/certs"

   # 使用自签证书 anytls.local
   RUST_LOG=info,anytls=debug \
   cargo run --release --bin anytls-server -- \
     -l "${LISTEN_ADDR}" \
     -p "${PASSWORD}" \
     --cert "${CERT_DIR}/anytls.local.crt" \
     --key "${CERT_DIR}/anytls.local.key"
   ```

   > 已预生成 `anytls.local` 自签证书；如需自定义域名，请使用 `rcgen` 或 openssl 生成新的 PEM 文件，并通过 `--cert/--key` 指定。

2. **编辑 sing-box 配置并校验**

   ```bash
   cp outbound-anytls.jsonc outbound-anytls.local.jsonc
   # 将 SERVER_HOST / PASSWORD 等字段替换为实际值
   sing-box check -c outbound-anytls.local.jsonc
   ```

3. **启动 sing-box outbound**

   ```bash
   # 确保 tls.server_name = "anytls.local" 且 certificate_path 指向上述证书
   sing-box run --config outbound-anytls.local.jsonc
   ```

4. **验证 TCP/UDP**

   ```bash
   # TCP：经 sing-box -> anytls-rs -> 目标 HTTP
   curl --socks5-hostname 127.0.0.1:61080 http://httpbin.org/get
   ```

   > UDP-over-TCP 调试：当前示例仅启用 SOCKS5 入站，可通过 SOCKS5 支持的应用测试 UDP。如需独立 DNS inbound，请在配置中添加 `dns` 入站并放通 `udp` 路由。

5. **自动化验证（可选）**

   ```bash
   ./scripts/dev-verify.sh
   ```

   该脚本会启动服务端、客户端并执行一次 `curl --socks5-hostname` 探测，完成后自动回收进程。

6. **可选：HTTP 代理**

   ```bash
   HTTP_ADDR=127.0.0.1:8080 ./scripts/dev-up.sh
   curl --proxy http://127.0.0.1:8080 http://httpbin.org/get
   ```

## 5. 配置字段对照（节选）

| sing-box 字段 | anytls-rs 映射 | 说明 |
|---------------|----------------|------|
| `password` | `anytls-server -p` | 必填，需保持一致 |
| `idle_session_check_interval` | `anytls-client -I/--idle-session-check-interval` | 秒级，默认 30 |
| `idle_session_timeout` | `anytls-client -T/--idle-session-timeout` | 秒级，默认 60 |
| `min_idle_session` | `anytls-client -M/--min-idle-session` | 默认 1 |
| `tls.enabled` | 恒为 `true`（自签） | 可将 `tls.insecure` 设为 `true` 接受自签 |
| `tls.server_name` | 暂未校验 | 可填写 `anytls.local` 等占位值 |

> 更多字段请参考 `docs/02-feature-mvp-plan.md` 与 sing-box 官方文档。服务端可通过 `anytls-server --idle-session-*` 选项下发建议值，与上述字段保持一致。

## 6. 常见问题

见 `docs/FAQ.md` 与 `docs/TROUBLESHOOTING.md`。

