# 开发者快速上手（AnyTLS-RS）

## 1. 前置条件

- Rust 工具链：`rustc`/`cargo` ≥ 1.70（建议使用 stable 最新版；项目基于 Edition 2024）。
- 可选依赖：
  - `openssl` CLI（手动生成服务器证书时使用）。
  - `mkcert` 或 `rcgen`（脚本内置自签证书生成）。
  - `curl`（本地验证 SOCKS5）。
  - `make` / `task` / `docker`（视自动化方案而定）。

环境检查：

```bash
rustup show
cargo --version
curl --version
```

## 2. 最短操作路径

- 构建：

  ```bash
  cargo build --release
  ```

- 运行（两个终端，默认开启 SOCKS5）：

  ```bash
  # 终端 A：服务端（免证书模式）
  cargo run --release --bin anytls-server -- \
    -l 127.0.0.1:8443 -p testpass

  # 如需使用已有证书（PEM）
  cargo run --release --bin anytls-server -- \
    -l 127.0.0.1:8443 -p testpass \
    --cert /path/to/server.crt \
    --key /path/to/server.key

  # 调整服务端推荐给客户端的会话池参数（单位：秒 / 个）
  cargo run --release --bin anytls-server -- \
    -l 127.0.0.1:8443 -p testpass \
    -I 20 \
    -T 90 \
    -M 2

  # 终端 B：客户端（内置 SOCKS5）
  cargo run --release --bin anytls-client -- \
    -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p testpass
  ```

- 验证（第三个终端）：

  ```bash
  curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
  ```

- 一键脚本（server + client）

  ```bash
  # 默认使用 examples/singbox/certs/anytls.local.{crt,key}，若不存在则回退为自签
  ./scripts/dev-up.sh
  ```

- 启用 HTTP 代理：

  ```bash
  # 额外监听 HTTP 代理（默认 127.0.0.1:8080，可通过 HTTP_ADDR 覆盖）
  HTTP_ADDR=127.0.0.1:8080 ./scripts/dev-up.sh
  ```

- 会话池参数（客户端）：

  ```bash
  IDLE_SESSION_CHECK_INTERVAL=20 \
  IDLE_SESSION_TIMEOUT=90 \
  MIN_IDLE_SESSION=2 \
  ./scripts/dev-up.sh
  ```

  ```bash
  # 服务端同步下发推荐配置
  SERVER_IDLE_SESSION_CHECK_INTERVAL=20 \
  SERVER_IDLE_SESSION_TIMEOUT=90 \
  SERVER_MIN_IDLE_SESSION=2 \
  ./scripts/dev-up.sh
  ```

  ```bash
  # 或手动指定短参数
  cargo run --release --bin anytls-client -- \
    -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p testpass \
    -I 20 -T 90 -M 2 \
    -H 127.0.0.1:8080
  ```

- 自动化校验（启动、探测、回收）：

  ```bash
  ./scripts/dev-verify.sh
  ```

- 最短测试：

  ```bash
  cargo test --tests
  ```

## 3. 常见坑与排查

| 问题 | 现象 | 排查建议 |
|------|------|----------|
| 端口占用 | 启动时报 `Address already in use` | `lsof -i :8443` / `lsof -i :1080`，或调整 `-l host:port` |
| TLS 证书路径无效 | 启动时报 `No such file or directory` | 核对 `--cert/--key` 路径；使用脚本生成时确认输出目录 |
| 密码不匹配 | 客户端握手失败，日志 `authentication failed` | 确认 `--password`；设置 `RUST_LOG=info,anytls=debug` 查看详情 |
| 防火墙阻断 | 客户端无法连接 | 本地测试建议监听 `127.0.0.1`；跨主机部署确保端口放行 |
| 日志不足 | 排查困难 | 启动前设置 `RUST_LOG=info,anytls=debug` |
| UDP-over-TCP 验证困难 | UDP 转发丢包或高延迟 | 使用 v0.3 组件；参考 `tests/udp` 和 `sing-box/protocol/anytls` 用例 |

## 4. 最小脚本 / 自动化补丁（建议）

### 4.1 新增 `scripts/dev-up.sh`

```diff
diff --git a/scripts/dev-up.sh b/scripts/dev-up.sh
new file mode 100755
--- /dev/null
+++ b/scripts/dev-up.sh
@@
+#!/usr/bin/env bash
+set -euo pipefail
+
+PASSWORD=${PASSWORD:-testpass}
+SERVER_ADDR=${SERVER_ADDR:-127.0.0.1:8443}
+CLIENT_ADDR=${CLIENT_ADDR:-127.0.0.1:1080}
+
+kill_existing() {
+  pkill -f "anytls-server" || true
+  pkill -f "anytls-client" || true
+}
+
+start_server() {
+  RUST_LOG=${RUST_LOG:-info,anytls=debug} \
+  cargo run --release --bin anytls-server -- \
+    -l "${SERVER_ADDR}" -p "${PASSWORD}" &
+  SERVER_PID=$!
+}
+
+start_client() {
+  RUST_LOG=${RUST_LOG:-info,anytls=debug} \
+  cargo run --release --bin anytls-client -- \
+    -l "${CLIENT_ADDR}" -s "${SERVER_ADDR}" -p "${PASSWORD}" &
+  CLIENT_PID=$!
+}
+
+teardown() {
+  kill "${SERVER_PID}" "${CLIENT_PID}" 2>/dev/null || true
+}
+
+trap teardown EXIT
+kill_existing
+start_server
+sleep 1
+start_client
+wait
```

### 4.2 可选 `Makefile` 片段

```diff
diff --git a/Makefile b/Makefile
new file mode 100644
--- /dev/null
+++ b/Makefile
@@
+dev-up:
+	@PASSWORD=${PASSWORD} SERVER_ADDR=${SERVER_ADDR} CLIENT_ADDR=${CLIENT_ADDR} \
+		bash scripts/dev-up.sh
+
+dev-test:
+	cargo test --tests
```

> 若不希望新增 `Makefile`，可改用 `Taskfile.yml` 或 `docker-compose.yml` 复用同一脚本逻辑。

