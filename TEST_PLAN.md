# AnyTLS-RS 端到端测试计划

## 测试目标

验证客户端和服务器的完整功能，包括：
1. 服务器启动和监听
2. 客户端启动和SOCKS5服务
3. TLS连接建立
4. 认证流程
5. SOCKS5代理转发
6. 数据完整性

---

## 测试环境准备

### 1. 编译二进制文件
```bash
cd anytls-rs
cargo build --release --bins
```

生成的二进制文件位置：
- `target/release/anytls-server`
- `target/release/anytls-client`

### 2. 准备测试工具
- `curl` - HTTP请求测试
- `nc` (netcat) - TCP连接测试
- `socat` - SOCKS5客户端（可选）

---

## 测试步骤

### 阶段1: 服务器启动测试

#### 测试 1.1: 服务器正常启动
```bash
# 终端1: 启动服务器
cd anytls-rs
cargo run --bin anytls-server -- \
  -l 127.0.0.1:8443 \
  -p test_password
```

**预期结果**:
- 服务器成功启动
- 输出: `[Server] Listening on 127.0.0.1:8443`
- 无错误信息

#### 测试 1.2: 服务器监听端口验证
```bash
# 验证端口是否监听
netstat -an | grep 8443
# 或
lsof -i :8443
```

**预期结果**:
- 端口 8443 处于 LISTEN 状态

---

### 阶段2: 客户端启动测试

#### 测试 2.1: 客户端正常启动
```bash
# 终端2: 启动客户端（保持服务器运行）
cd anytls-rs
cargo run --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -s 127.0.0.1:8443 \
  -p test_password
```

**预期结果**:
- 客户端成功启动
- 输出: `[Client] SOCKS5/HTTP 127.0.0.1:1080 => 127.0.0.1:8443`
- 输出: `[SOCKS5] Listening on 127.0.0.1:1080`
- 无错误信息

#### 测试 2.2: 客户端SOCKS5端口验证
```bash
# 验证SOCKS5端口
netstat -an | grep 1080
# 或
lsof -i :1080
```

**预期结果**:
- 端口 1080 处于 LISTEN 状态

---

### 阶段3: 连接建立测试

#### 测试 3.1: TLS握手测试
观察服务器和客户端的日志输出，检查：
- 客户端是否成功建立TLS连接
- 认证是否成功
- Settings命令是否发送和接收

**预期日志** (客户端):
```
[Client] Client created successfully
[SOCKS5] Listening on 127.0.0.1:1080
[SOCKS5] New connection from ...
```

**预期日志** (服务器):
```
[Server] New connection from ...
[Server] Connection established
```

#### 测试 3.2: 协议版本协商
观察日志，检查：
- Settings命令中的版本号
- ServerSettings响应
- PaddingScheme更新（如果MD5不匹配）

---

### 阶段4: SOCKS5代理功能测试

#### 测试 4.1: SOCKS5握手测试

**方法1: 使用curl（推荐）**
```bash
# 等待服务器和客户端都启动后（约3-5秒）
curl -v --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
```

**方法2: 手动SOCKS5握手**
```bash
# 使用nc测试SOCKS5握手
(echo -ne "\x05\x01\x00"; sleep 1) | nc 127.0.0.1 1080 | od -An -tx1
```

**预期结果**:
- curl: 返回HTTP响应（JSON格式）
- nc: 收到 `05 00` (NO AUTHENTICATION)

#### 测试 4.2: 本地HTTP服务器测试
```bash
# 终端3: 启动简单的HTTP服务器
python3 -m http.server 8080

# 终端4: 使用curl通过SOCKS5代理访问
curl --socks5-hostname 127.0.0.1:1080 http://127.0.0.1:8080/
```

**预期结果**:
- 能够通过代理访问本地HTTP服务器
- 返回HTTP响应内容

#### 测试 4.3: 外网HTTP请求测试
```bash
# 通过SOCKS5代理访问外网
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
```

**预期结果**:
- 能够成功获取响应
- 响应内容正确

---

### 阶段5: 数据完整性测试

#### 测试 5.1: 大数据传输测试
```bash
# 创建一个大文件
dd if=/dev/zero of=test_large.bin bs=1M count=10

# 通过代理下载（使用支持SOCKS5的工具）
```

#### 测试 5.2: 并发连接测试
```bash
# 同时发起多个请求
for i in {1..5}; do
  curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get &
done
wait
```

**预期结果**:
- 所有请求都能成功完成
- 服务器和客户端都能正确处理并发连接

---

### 阶段6: 错误处理测试

#### 测试 6.1: 错误密码测试
```bash
# 使用错误的密码启动客户端
cargo run --bin anytls-client -- \
  -l 127.0.0.1:1081 \
  -s 127.0.0.1:8443 \
  -p wrong_password
```

**预期结果**:
- 认证失败
- 客户端报告错误
- 连接被关闭

#### 测试 6.2: 无效目标地址测试
通过SOCKS5代理访问无效地址：
```bash
curl --socks5-hostname 127.0.0.1:1080 http://invalid-host-that-does-not-exist.test:80
```

**预期结果**:
- 适当的错误处理
- 不会崩溃

---

## 自动化测试脚本

### 简化测试脚本 (test_e2e.sh)

```bash
#!/bin/bash
set -e

PASSWORD="test_password"
SERVER_PORT=8443
CLIENT_PORT=1080
TEST_URL="http://httpbin.org/get"

echo "=== AnyTLS-RS 端到端测试 ==="

# 1. 编译
echo "[1/6] 编译二进制文件..."
cargo build --release --bins

# 2. 启动服务器（后台）
echo "[2/6] 启动服务器..."
cargo run --release --bin anytls-server -- \
  -l "127.0.0.1:$SERVER_PORT" \
  -p "$PASSWORD" > server.log 2>&1 &
SERVER_PID=$!
sleep 2

# 3. 启动客户端（后台）
echo "[3/6] 启动客户端..."
cargo run --release --bin anytls-client -- \
  -l "127.0.0.1:$CLIENT_PORT" \
  -s "127.0.0.1:$SERVER_PORT" \
  -p "$PASSWORD" > client.log 2>&1 &
CLIENT_PID=$!
sleep 3

# 4. 测试SOCKS5连接
echo "[4/6] 测试SOCKS5代理..."
if curl -s --socks5-hostname "127.0.0.1:$CLIENT_PORT" "$TEST_URL" > /dev/null; then
    echo "✓ SOCKS5代理测试通过"
else
    echo "✗ SOCKS5代理测试失败"
    exit 1
fi

# 5. 清理
echo "[5/6] 清理进程..."
kill $CLIENT_PID 2>/dev/null || true
kill $SERVER_PID 2>/dev/null || true
sleep 1

# 6. 检查日志
echo "[6/6] 检查日志..."
if grep -q "error\|Error\|ERROR" server.log client.log; then
    echo "⚠ 发现错误日志"
    grep -i "error" server.log client.log
else
    echo "✓ 无错误日志"
fi

echo "=== 测试完成 ==="
```

---

## 手动测试清单

- [ ] 服务器能够成功启动
- [ ] 客户端能够成功启动
- [ ] TLS连接建立成功
- [ ] 认证成功
- [ ] SOCKS5握手成功
- [ ] 能够通过代理访问本地服务
- [ ] 能够通过代理访问外网
- [ ] 数据转发正确
- [ ] 并发连接正常
- [ ] 错误处理正确

---

## 调试技巧

### 启用详细日志
```bash
RUST_LOG=debug cargo run --bin anytls-server -- -l 127.0.0.1:8443 -p test_password
RUST_LOG=debug cargo run --bin anytls-client -- -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

### 使用tcpdump查看流量
```bash
# 监听服务器端口
sudo tcpdump -i lo0 -A 'port 8443'

# 监听客户端端口
sudo tcpdump -i lo0 -A 'port 1080'
```

### 检查进程状态
```bash
ps aux | grep anytls
```

---

*最后更新: 2025-01-XX*

