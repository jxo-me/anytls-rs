# AnyTLS-RS 手动测试指南

## 快速开始

### 1. 编译项目
```bash
cd anytls-rs
cargo build --release --bins
```

### 2. 启动服务器
```bash
# 终端1
cargo run --release --bin anytls-server -- -l 127.0.0.1:8443 -p test_password
```

**预期输出**:
```
[Server] anytls-rs v0.1.0
[Server] Listening TCP 127.0.0.1:8443
[Server] Listening on 127.0.0.1:8443
```

### 3. 启动客户端
```bash
# 终端2
cargo run --release --bin anytls-client -- -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

**预期输出**:
```
[Client] anytls-rs v0.1.0
[Client] SOCKS5/HTTP 127.0.0.1:1080 => 127.0.0.1:8443
[Client] Client created successfully
[SOCKS5] Listening on 127.0.0.1:1080
```

### 4. 测试SOCKS5代理
```bash
# 终端3 - 测试HTTP请求
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
```

**预期结果**:
- 返回JSON响应，包含请求信息

---

## 详细测试步骤

### 测试1: 基本连通性

```bash
# 检查端口监听
lsof -i :8443  # 服务器
lsof -i :1080  # 客户端SOCKS5
```

### 测试2: SOCKS5协议测试

#### 2.1 简单HTTP请求
```bash
curl -v --socks5-hostname 127.0.0.1:1080 http://httpbin.org/ip
```

#### 2.2 HTTPS请求
```bash
curl -v --socks5-hostname 127.0.0.1:1080 https://httpbin.org/get
```

#### 2.3 检查IP地址（验证代理工作）
```bash
# 通过代理访问
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/ip

# 对比：直接访问
curl http://httpbin.org/ip
```

### 测试3: 本地服务测试

```bash
# 启动本地HTTP服务器
python3 -m http.server 8080

# 通过代理访问
curl --socks5-hostname 127.0.0.1:1080 http://127.0.0.1:8080/
```

### 测试4: 并发测试

```bash
# 同时发起多个请求
for i in {1..10}; do
    curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get &
done
wait
```

### 测试5: 大数据传输

```bash
# 下载大文件测试
curl --socks5-hostname 127.0.0.1:1080 -o /tmp/test.bin \
    http://speedtest.tele2.net/100MB.zip
```

### 测试6: 错误处理

#### 6.1 错误密码
```bash
cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1081 \
  -s 127.0.0.1:8443 \
  -p wrong_password
```
**预期**: 认证失败，连接关闭

#### 6.2 无效目标地址
```bash
curl --socks5-hostname 127.0.0.1:1080 \
    http://invalid-host-that-does-not-exist.test:80
```
**预期**: 适当的错误处理，不会崩溃

---

## 调试模式

### 启用详细日志

```bash
# 服务器
RUST_LOG=debug cargo run --bin anytls-server -- -l 127.0.0.1:8443 -p test_password

# 客户端
RUST_LOG=debug cargo run --bin anytls-client -- -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

### 查看网络流量

```bash
# 监听服务器端口
sudo tcpdump -i lo0 -A 'port 8443'

# 监听客户端SOCKS5端口
sudo tcpdump -i lo0 -A 'port 1080'
```

---

## 测试检查清单

### 功能测试
- [ ] 服务器成功启动
- [ ] 客户端成功启动
- [ ] TLS握手成功
- [ ] 认证成功
- [ ] SOCKS5握手成功
- [ ] HTTP请求通过代理
- [ ] HTTPS请求通过代理
- [ ] 本地服务访问正常
- [ ] 外网服务访问正常

### 性能测试
- [ ] 单个请求响应时间 < 2秒
- [ ] 并发10个请求都能成功
- [ ] 大数据传输正常（>10MB）

### 稳定性测试
- [ ] 长时间运行无崩溃（>5分钟）
- [ ] 内存使用稳定
- [ ] 错误处理正确

---

## 常见问题

### Q: TLS握手失败 "UnknownCA"
**A**: 客户端已配置为接受自签名证书，应该能正常工作。如果仍然失败，检查日志。

### Q: SOCKS5连接被拒绝
**A**: 确保客户端SOCKS5服务已启动，端口1080正在监听。

### Q: 代理请求超时
**A**: 
1. 检查服务器和客户端是否都在运行
2. 检查网络连接
3. 查看日志中的错误信息

---

*最后更新: 2025-01-XX*

