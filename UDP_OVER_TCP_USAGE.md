# UDP over TCP 使用指南

**协议版本**: sing-box udp-over-tcp v2 (Connect format)  
**实现状态**: ✅ 完成 (服务器端 + 客户端)

---

## 📋 功能概述

UDP over TCP 允许通过 TCP 连接传输 UDP 数据包，适用于以下场景：

- DNS 查询代理
- VoIP 通话代理
- UDP 游戏流量代理
- QUIC 协议代理

---

## 🔧 服务器端配置

服务器端**无需额外配置**，自动支持 UDP over TCP。

当客户端请求目标地址包含 `udp-over-tcp.arpa` 时，服务器自动切换到 UDP 代理模式。

### 检测机制

```rust
// 在 handler.rs 中自动检测
if destination.addr.contains("udp-over-tcp.arpa") {
    tracing::info!("[Proxy] Detected UDP over TCP request");
    handle_udp_over_tcp(stream).await
}
```

---

## 💻 客户端使用

### 方法 1: 使用 Client API

```rust
use anytls_rs::Client;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建客户端
    let client = Arc::new(Client::new(
        "your-password",
        "server.example.com:8443".to_string(),
        tls_config,
        padding,
    ));
    
    // 创建 UDP over TCP 代理
    // 目标: 8.8.8.8:53 (Google DNS)
    let local_addr = client.create_udp_proxy(
        "127.0.0.1:0",  // 本地监听地址 (0 = 随机端口)
        "8.8.8.8:53".parse()?  // 目标 UDP 服务器
    ).await?;
    
    println!("UDP proxy listening on: {}", local_addr);
    
    // 现在可以向 local_addr 发送 UDP 数据包
    // 它们会通过 AnyTLS 转发到 8.8.8.8:53
    
    Ok(())
}
```

### 方法 2: 通过 SOCKS5 (TODO)

```bash
# 未来版本将支持 SOCKS5 UDP ASSOCIATE
curl --socks5 127.0.0.1:1080 udp://8.8.8.8:53
```

---

## 📦 协议格式

### 初始请求

客户端发送到服务器的第一个数据包：

```
| isConnect | ATYP | Address | Port |
| 0x01      | 0x01 | 8.8.8.8 | 53   |
|  1 byte   |1 byte| 4 bytes |2 bytes|
```

- **isConnect**: 固定为 `1` (Connect 格式)
- **ATYP**: SOCKS5 地址类型
  - `0x01` = IPv4
  - `0x03` = Domain
  - `0x04` = IPv6
- **Address**: 目标地址
- **Port**: 目标端口 (Big-Endian)

### 数据包格式

后续每个 UDP 数据包：

```
| Length | UDP Data     |
| 2 bytes| variable     |
```

- **Length**: UDP 数据长度 (Big-Endian uint16)
- **UDP Data**: 实际的 UDP 数据

---

## 🧪 测试示例

### 单元测试

```bash
# 运行所有 UDP over TCP 测试
cargo test udp

# 服务器端测试
cargo test --lib server::udp_proxy

# 客户端测试
cargo test --lib client::udp_client
```

### 集成测试

```rust
use anytls_rs::*;

#[tokio::test]
async fn test_udp_dns_query() {
    let client = create_test_client().await;
    
    // 创建到 Google DNS 的 UDP 代理
    let local_addr = client.create_udp_proxy(
        "127.0.0.1:0",
        "8.8.8.8:53".parse().unwrap()
    ).await.unwrap();
    
    // 发送 DNS 查询
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(DNS_QUERY, local_addr).await.unwrap();
    
    // 接收 DNS 响应
    let mut buf = [0u8; 512];
    let (len, _) = socket.recv_from(&mut buf).await.unwrap();
    
    assert!(len > 0);
}
```

---

## 🎯 使用场景

### 场景 1: DNS 代理

```rust
// 代理 Google DNS
client.create_udp_proxy(
    "127.0.0.1:5353",
    "8.8.8.8:53".parse()?
).await?;

// 配置系统 DNS 为 127.0.0.1:5353
```

### 场景 2: 游戏流量代理

```rust
// 代理游戏服务器 UDP 流量
client.create_udp_proxy(
    "127.0.0.1:0",
    "game-server.com:27015".parse()?
).await?;
```

### 场景 3: VoIP 通话

```rust
// 代理 SIP/RTP 流量
client.create_udp_proxy(
    "127.0.0.1:5060",
    "sip-server.com:5060".parse()?
).await?;
```

---

## ⚠️ 注意事项

### 1. 性能考虑

- UDP over TCP 会增加约 20-30% 的延迟
- 不适合极低延迟需求的场景 (如实时游戏)
- 适合对可靠性要求高的场景

### 2. MTU 限制

- 最大 UDP 包大小: 65535 字节
- 建议保持在 1500 字节以内以避免分片

### 3. 连接管理

- 每个 UDP 目标需要一个独立的 Stream
- 空闲连接会自动清理 (TODO: 实现超时机制)

---

## 🔍 故障排查

### 问题 1: 连接失败

```
[UDP Client] Failed to create stream
```

**解决方案**:
- 检查服务器地址和端口
- 确保 TLS 证书有效
- 检查网络连接

### 问题 2: 数据包丢失

```
[UDP Client] Failed to send to stream: Channel send failed
```

**解决方案**:
- 检查 Stream 是否关闭
- 增加日志级别查看详细错误

### 问题 3: 协议不兼容

```
[UDP] Unsupported UDP over TCP format: isConnect=0
```

**解决方案**:
- 确保客户端使用 Connect 格式 (isConnect=1)
- 检查协议版本是否匹配

---

## 📚 参考资料

- [sing-box UDP over TCP v2 协议](https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md)
- [SOCKS5 RFC 1928](https://tools.ietf.org/html/rfc1928)
- [UDP RFC 768](https://tools.ietf.org/html/rfc768)

---

## 🚀 未来计划

- [ ] SOCKS5 UDP ASSOCIATE 支持
- [ ] 自动连接池管理
- [ ] UDP 会话超时机制
- [ ] 性能优化 (零拷贝)
- [ ] 多路复用优化

---

*最后更新: 2025-11-03*

