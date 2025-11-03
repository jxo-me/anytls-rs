# AnyTLS-RS 测试结果记录

## 测试日期
2025-11-02

## 测试环境
- Rust版本: `rustc --version`
- 操作系统: macOS (darwin 25.0.0)
- 测试工具: curl, netcat

## 测试步骤

### 1. 启动服务器
```bash
cargo run --release --bin anytls-server -- -l 127.0.0.1:8443 -p test_password
```

### 2. 启动客户端
```bash
cargo run --release --bin anytls-client -- -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

### 3. 测试SOCKS5代理
```bash
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
```

## 预期行为
- 服务器和客户端都能成功启动
- TLS握手成功（无UnknownCA错误）
- SOCKS5代理能够转发HTTP请求
- 返回正确的HTTP响应

## 实际结果
（待填写）

## 问题记录
（待填写）

