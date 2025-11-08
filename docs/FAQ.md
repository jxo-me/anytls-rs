# FAQ：sing-box outbound 对接 anytls-rs

## Q1：anytls-rs 服务端是否必须配置证书路径？

自 v0.3.2 起支持 `--cert` / `--key`（PEM）。若未提供则自动生成自签证书。确保两者成对使用，否则会报错。

## Q2：sing-box 配置中的 `tls.insecure` 必须开启吗？

若使用默认自签证书，需要在 sing-box 中设置 `"tls.insecure": true` 或提供 `certificate` 内容。上线环境建议提供受信 CA 证书，并扩展 `anytls-server` 以加载证书文件。

## Q3：如何调节会话池参数？

客户端支持以下 CLI 参数（及对应脚本环境变量），服务端可通过同名选项向客户端下发建议值：

- `-I, --idle-session-check-interval <秒>` / `IDLE_SESSION_CHECK_INTERVAL`（默认 30）
- `-T, --idle-session-timeout <秒>` / `IDLE_SESSION_TIMEOUT`（默认 60）
- `-M, --min-idle-session <整数>` / `MIN_IDLE_SESSION`（默认 1）

示例：

```bash
cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -s server.example.com:8443 \
  -p your_password \
  -I 20 \
  -T 90 \
  -M 2
```

## Q4：如何观察握手与心跳？

设置环境变量：

```bash
RUST_LOG=info,anytls=debug cargo run --release --bin anytls-server ...
```

未来计划新增结构化 span，输出 `session_id`、`stream_id`、`bytes_in/out` 等字段。

## Q5：UDP-over-TCP 是否默认开启？

是的。v0.3 客户端与服务端默认支持 UDP-over-TCP。sing-box 配置中仅需在 `route.rules` 允许 `network: tcp,udp` 即可。

## Q6：如何启用 HTTP 代理？

`anytls-client` 支持 `-H/--http-listen` 参数，同时启动 HTTP CONNECT/明文代理。例如：

```bash
cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -H 127.0.0.1:8080 \
  -s server.example.com:8443 \
  -p your_password
```

脚本 `HTTP_ADDR=127.0.0.1:8080 ./scripts/dev-up.sh` 也会自动开启该功能。

