# 故障排查指南

## 🔍 问题：SOCKS5连接被关闭

### 症状
```
curl: (97) connection to proxy closed
```

### 可能原因和排查步骤

#### 1. 检查客户端日志

```bash
# 使用debug级别日志
RUST_LOG=debug ./anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password
```

**关键日志信息**：
- `[SOCKS5] New connection from` - SOCKS5客户端连接
- `[SOCKS5] Creating proxy stream` - 开始创建代理流
- `[SOCKS5] Proxy stream created successfully` - 流创建成功
- `[SOCKS5] Failed to create proxy stream` - 流创建失败

#### 2. 检查服务器日志

```bash
RUST_LOG=debug ./anytls-server -l 0.0.0.0:8443 -p password
```

**关键日志信息**：
- `[Server] New connection from` - 客户端TLS连接
- `[Server] Connection established` - 连接建立
- `[Proxy] Connecting to` - 开始代理连接
- `[Proxy] Connection error` - 代理连接错误

#### 3. 常见问题

##### 问题A: TLS握手失败

**症状**: 客户端日志显示 "TLS handshake failed"

**原因**: 
- 证书验证问题（应该已修复，接受自签名证书）
- 服务器未启动或端口错误

**解决**:
```bash
# 检查服务器是否运行
netstat -an | grep 8443  # 或 ss -an | grep 8443

# 检查网络连通性
telnet server_ip 8443
```

##### 问题B: 认证失败

**症状**: 服务器日志显示认证错误

**原因**: 密码不匹配

**解决**: 确保客户端和服务器使用相同的密码

##### 问题C: Stream创建失败

**症状**: 客户端日志显示 "Failed to create proxy stream"

**可能原因**:
1. Session未正确启动
2. SYN帧发送失败
3. 服务器未正确处理SYN

**排查**:
```bash
# 检查服务器是否收到SYN帧
RUST_LOG=debug ./anytls-server ...

# 检查客户端Session状态
# 在代码中添加更多日志
```

##### 问题D: 数据转发失败

**症状**: SOCKS5握手成功，但连接立即关闭

**可能原因**:
1. Stream读取/写入失败
2. 服务器端代理连接失败
3. 网络问题

**排查**:
```bash
# 检查服务器端代理连接
# 服务器应该尝试连接到目标地址

# 检查Stream状态
# 添加更多调试日志
```

---

## 🛠️ 调试技巧

### 1. 启用详细日志

```bash
# 客户端
RUST_LOG=trace ./anytls-client ...

# 服务器
RUST_LOG=trace ./anytls-server ...
```

### 2. 使用网络抓包

```bash
# 监听客户端SOCKS5端口
sudo tcpdump -i any -A 'port 1080'

# 监听服务器端口
sudo tcpdump -i any -A 'port 8443'
```

### 3. 检查进程状态

```bash
# 检查端口监听
netstat -tulpn | grep -E "1080|8443"
# 或
ss -tulpn | grep -E "1080|8443"

# 检查进程
ps aux | grep anytls
```

### 4. 测试连接

```bash
# 测试服务器TCP连接
telnet server_ip 8443

# 测试客户端SOCKS5连接
curl -v --socks5-hostname client_ip:1080 http://httpbin.org/get
```

---

## 📝 添加调试日志

如果问题仍然存在，可以在关键位置添加日志：

### 客户端 - SOCKS5处理

```rust
// src/client/socks5.rs
tracing::debug!("[SOCKS5] Creating proxy stream to {}:{}", addr, port);
tracing::debug!("[SOCKS5] Proxy stream created, starting data forwarding");
tracing::error!("[SOCKS5] Data forwarding error: {:?}", e);
```

### 服务器 - Stream处理

```rust
// src/server/handler.rs
tracing::debug!("[Proxy] New stream received");
tracing::debug!("[Proxy] Parsing SOCKS5 address");
tracing::debug!("[Proxy] Connecting to {}:{}", addr, port);
```

---

## 🎯 快速检查清单

- [ ] 服务器正在运行并监听8443端口
- [ ] 客户端正在运行并监听1080端口
- [ ] 密码匹配
- [ ] 网络连通性正常（可以telnet服务器端口）
- [ ] 查看debug日志了解具体失败点
- [ ] 检查是否有TLS/认证错误
- [ ] 检查Stream创建是否成功

---

*最后更新: 2025-11-02*

