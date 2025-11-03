# AnyTLS-RS 测试指南

## 🎯 测试目标

验证 AnyTLS-RS 实现的完整功能：
1. ✅ TLS证书生成（已完成）
2. ✅ 服务器代理转发（已完成）
3. ✅ 客户端SOCKS5服务（已完成）
4. ✅ 协议完善（Settings/PaddingScheme等，已完成）
5. ⏳ **端到端功能验证（当前步骤）**

---

## 📝 测试步骤详解

### 步骤1: 编译项目

```bash
cd anytls-rs
cargo build --release --bins
```

**验证**: 
- `target/release/anytls-server` 文件存在
- `target/release/anytls-client` 文件存在

---

### 步骤2: 启动服务器（终端1）

```bash
cd anytls-rs
RUST_LOG=info cargo run --release --bin anytls-server -- \
  -l 127.0.0.1:8443 \
  -p test_password
```

**预期输出**:
```
[Server] anytls-rs v0.1.0
[Server] Listening TCP 127.0.0.1:8443
[Server] Listening on 127.0.0.1:8443
```

**检查点**:
- [ ] 服务器成功启动，无错误
- [ ] 端口8443正在监听：`lsof -i :8443`

---

### 步骤3: 启动客户端（终端2）

```bash
cd anytls-rs
RUST_LOG=info cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -s 127.0.0.1:8443 \
  -p test_password
```

**预期输出**:
```
[Client] anytls-rs v0.1.0
[Client] SOCKS5/HTTP 127.0.0.1:1080 => 127.0.0.1:8443
[Client] Client created successfully
[SOCKS5] Listening on 127.0.0.1:1080
```

**检查点**:
- [ ] 客户端成功启动，无错误
- [ ] 端口1080正在监听：`lsof -i :1080`
- [ ] 观察服务器日志，确认TLS握手和认证成功

**重要**: 等待3-5秒让客户端建立到服务器的初始连接

---

### 步骤4: 测试SOCKS5代理（终端3）

#### 测试4.1: 基本HTTP请求

```bash
# 等待客户端建立连接后（约5秒），执行：
curl -v --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
```

**预期结果**:
```
* Connected to 127.0.0.1 (127.0.0.1) port 1080
* SOCKS5 connect to httpbin.org:80 (remotely resolved)
> GET /get HTTP/1.1
...
{
  "args": {},
  "headers": {
    ...
  },
  "origin": "...",
  "url": "http://httpbin.org/get"
}
```

**检查点**:
- [ ] curl成功连接SOCKS5代理
- [ ] 返回有效的JSON响应
- [ ] HTTP状态码200

---

#### 测试4.2: 验证代理工作

```bash
# 通过代理获取IP地址
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/ip

# 对比：直接访问
curl http://httpbin.org/ip
```

**预期结果**: 两个命令返回的IP地址应该不同（代理的IP vs 本地IP）

---

#### 测试4.3: HTTPS请求

```bash
curl -v --socks5-hostname 127.0.0.1:1080 https://httpbin.org/get
```

**预期结果**: 成功返回HTTPS响应

---

#### 测试4.4: 本地服务测试

```bash
# 终端4: 启动本地HTTP服务器
python3 -m http.server 8080

# 终端3: 通过代理访问
curl --socks5-hostname 127.0.0.1:1080 http://127.0.0.1:8080/
```

**预期结果**: 返回本地服务器的响应

---

### 步骤5: 调试模式测试

如果步骤4失败，使用调试模式查看详细信息：

```bash
# 服务器（终端1）
RUST_LOG=debug cargo run --release --bin anytls-server -- \
  -l 127.0.0.1:8443 -p test_password

# 客户端（终端2）
RUST_LOG=debug cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

**关键日志信息**:
- `[Server] New connection from` - 客户端连接
- `[Client] Client created successfully` - 客户端创建
- `[SOCKS5] New connection from` - SOCKS5客户端连接
- `[Proxy] Connecting to` - 代理转发开始

---

## 🔍 故障排查

### 问题1: TLS握手失败 "UnknownCA"

**原因**: 客户端证书验证失败

**已修复**: 客户端已配置为接受自签名证书（`NoCertificateVerification`）

**验证**: 检查日志中是否仍有TLS错误

---

### 问题2: SOCKS5连接立即关闭

**可能原因**:
1. 客户端未建立到服务器的连接
2. 认证失败
3. Session未正确启动

**调试步骤**:
1. 检查服务器日志是否有连接记录
2. 检查客户端日志是否有TLS握手错误
3. 启用debug日志查看详细信息

---

### 问题3: curl超时

**可能原因**:
1. 服务器未启动
2. 客户端未启动
3. 网络问题

**调试步骤**:
```bash
# 检查端口监听
lsof -i :8443  # 服务器
lsof -i :1080  # 客户端

# 检查进程
ps aux | grep anytls
```

---

## 📊 测试检查清单

### 基础功能
- [ ] 服务器成功启动并监听端口
- [ ] 客户端成功启动并监听SOCKS5端口
- [ ] TLS连接建立成功（无UnknownCA错误）
- [ ] 认证成功
- [ ] Settings命令发送和接收

### SOCKS5功能
- [ ] SOCKS5握手成功
- [ ] HTTP请求通过代理
- [ ] HTTPS请求通过代理
- [ ] 本地服务访问正常
- [ ] 外网服务访问正常

### 稳定性
- [ ] 单个请求正常
- [ ] 多次请求正常
- [ ] 并发请求正常

---

## 📖 参考文档

- 测试计划和结果已包含在 `TEST_SUCCESS_REPORT.md` 和 `TEST_RESULTS.md` 中
- 手动测试说明已包含在本文档中
- `test_e2e.sh` - 自动化测试脚本
- `test_simple.sh` - 简化测试脚本

---

*最后更新: 2025-11-02*

