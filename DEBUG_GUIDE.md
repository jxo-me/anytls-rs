# 调试指南 - SOCKS5连接被关闭问题

## 🔴 问题现象

```
curl -v --socks5-hostname 192.168.100.10:1080 http://httpbin.org/get
* Connected to 192.168.100.10 (192.168.100.10) port 1080
* SOCKS5 connect to httpbin.org:80 (remotely resolved)
* connection to proxy closed
curl: (97) connection to proxy closed
```

这说明：
- ✅ SOCKS5握手成功
- ✅ 代理连接请求已发送
- ❌ 连接被过早关闭

---

## 🔍 排查步骤

### 步骤1: 启用详细日志

#### 客户端
```bash
RUST_LOG=debug ./anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password
```

或更详细：
```bash
RUST_LOG=trace ./anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password
```

#### 服务器
```bash
RUST_LOG=debug ./anytls-server -l 0.0.0.0:8443 -p password
```

---

### 步骤2: 查看关键日志

#### 客户端应该显示：
1. `[SOCKS5] New connection from` - SOCKS5客户端连接
2. `[SOCKS5] Creating proxy stream to httpbin.org:80` - 创建代理流
3. `[SOCKS5] Proxy stream created successfully` - 流创建成功（或失败）
4. `[SOCKS5] Sending success reply` - 发送成功响应
5. `[SOCKS5] Starting bidirectional data forwarding` - 开始数据转发

#### 如果失败，查看：
- `[SOCKS5] Failed to create proxy stream: ...` - 流创建失败原因
- `[Client] TLS handshake failed` - TLS握手失败
- `[Session] Stream ... error` - Stream错误

---

### 步骤3: 检查常见问题

#### 问题1: Stream创建失败

**日志特征**: `[SOCKS5] Failed to create proxy stream`

**可能原因**:
1. **TLS连接未建立**: 检查 `[Client] TLS handshake` 日志
2. **认证失败**: 检查密码是否匹配
3. **Session未启动**: 检查 `[Session]` 相关日志
4. **SYN帧发送失败**: 检查网络和Session状态

**排查**:
```bash
# 检查TLS连接
grep "TLS\|handshake\|auth" client.log

# 检查Session状态
grep "Session\|SYN\|stream" client.log
```

---

#### 问题2: 数据转发立即失败

**日志特征**: 
- `[SOCKS5] Starting bidirectional data forwarding`
- 立即出现 `[SOCKS5] Task1/Task2 finished` 或错误

**可能原因**:
1. **Stream读取立即返回EOF**: Stream可能未正确建立
2. **服务器端未处理SYN**: 检查服务器日志
3. **服务器端代理连接失败**: 检查 `[Proxy]` 相关日志

**排查**:
```bash
# 客户端日志
grep "SOCKS5.*Task\|proxy stream\|EOF\|Error" client.log

# 服务器日志
grep "Proxy\|SYN\|stream\|Connection" server.log
```

---

#### 问题3: 服务器端未收到SYN

**症状**: 客户端显示流创建成功，但服务器没有相应日志

**排查**:
```bash
# 服务器应该显示：
# [Server] New connection from ...
# [Proxy] New stream received
# [Proxy] Parsing SOCKS5 address
# [Proxy] Connecting to httpbin.org:80
```

如果服务器没有这些日志，说明：
- Session未正确启动
- 数据未正确传输
- 网络问题

---

## 🛠️ 代码修复

我已经添加了以下改进：

1. **错误处理改进**: 如果 `create_proxy_stream` 失败，会发送错误响应给SOCKS5客户端
2. **详细日志**: 添加了更多调试日志点
3. **错误级别提升**: 将关键错误从 `debug` 提升到 `error`

---

## 📋 下一步操作

1. **重新编译**:
```bash
cd anytls-rs
cargo build --release --bins
```

2. **启用详细日志测试**:
```bash
# 终端1 - 服务器
RUST_LOG=debug ./target/release/anytls-server -l 0.0.0.0:8443 -p password

# 终端2 - 客户端
RUST_LOG=debug ./target/release/anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password

# 终端3 - 测试
curl -v --socks5-hostname client_ip:1080 http://httpbin.org/get
```

3. **查看日志**: 根据日志信息定位具体失败点

---

## 💡 预期行为

### 正常流程日志示例：

**客户端**:
```
[SOCKS5] New connection from 192.168.100.10:xxxxx
[SOCKS5] Creating proxy stream to httpbin.org:80
[Client] Creating new session...
[Client] TLS handshake successful
[Session] Starting client session
[SOCKS5] Proxy stream created successfully
[SOCKS5] Sending success reply
[SOCKS5] Starting bidirectional data forwarding
[SOCKS5] Forwarded XXX bytes to proxy stream
[SOCKS5] Forwarded XXX bytes to SOCKS5 client
```

**服务器**:
```
[Server] New connection from 192.168.x.x:xxxxx
[Server] Connection established
[Proxy] New stream received
[Proxy] Parsing SOCKS5 address: httpbin.org:80
[Proxy] Connecting to httpbin.org:80
[Proxy] Connection established, starting forwarding
```

---

*最后更新: 2025-11-02*

