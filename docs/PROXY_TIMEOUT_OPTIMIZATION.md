# 代理超时优化文档

## 问题分析

### 1. 核心问题：无超时配置导致连接阻塞

**症状**：
- YouTube、Google 等网站打不开或加载极慢
- 客户端长时间无响应
- 某些网站完全无法访问

**根本原因**：
服务器端代理连接没有设置超时，导致：
- DNS 解析慢或失败时无限等待
- 目标网站无响应时长时间阻塞
- 资源无法释放，影响其他请求

### 2. 问题代码（优化前）

```rust
// ❌ 没有超时！
let outbound = TcpStream::connect(&target_addr).await?;
```

这会导致以下场景阻塞：
1. **DNS 污染/解析失败**：YouTube.com 等被污染的域名
2. **网络不可达**：目标服务器网络故障
3. **防火墙阻断**：某些端口被封
4. **慢速连接**：高延迟网络

### 3. 日志分析

您的日志显示的问题：

```log
# 正常的 TLS 扫描器（可忽略）
NoCipherSuitesInCommon  
Illegal SNI extension: ignoring IP address presented as hostname

# 真正的问题：无超时导致挂起
# 没有看到 "Connection timeout" 日志，说明在阻塞中
```

## 解决方案

### 1. TCP 连接超时（15秒）

**位置**：`src/server/handler.rs`

```rust
// ✅ 添加 15 秒超时
let connect_timeout = Duration::from_secs(15);
let outbound = match timeout(connect_timeout, TcpStream::connect(&target_addr)).await {
    Ok(Ok(conn)) => conn,  // 连接成功
    Ok(Err(e)) => {
        // TCP 连接失败（拒绝、网络错误等）
        return Err(...);
    }
    Err(_) => {
        // 超时（DNS 慢、无响应等）
        return Err(AnyTlsError::Protocol(
            format!("Connection timeout ({}s) to {}", 15, target_addr)
        ));
    }
};
```

**优点**：
- DNS 解析 + TCP 握手限制在 15 秒内
- 超时后立即返回错误给客户端
- 释放资源，不阻塞其他连接

### 2. DNS 解析超时（10秒）

**位置**：`src/server/udp_proxy.rs`

```rust
// ✅ DNS 单独设置 10 秒超时
let dns_timeout = Duration::from_secs(10);
let addr = match timeout(dns_timeout, tokio::net::lookup_host((domain, port))).await {
    Ok(Ok(mut addrs)) => addrs.next().unwrap(),
    Ok(Err(e)) => return Err(...),  // DNS 错误
    Err(_) => return Err(AnyTlsError::Protocol(
        format!("DNS resolution timeout ({}s) for {}", 10, domain)
    )),
};
```

### 3. 错误通知优化

超时后会通过 SYNACK 帧通知客户端具体错误：

```rust
if peer_version >= 2 {
    let error_msg = format!("Connection timeout (15s) to {}", target_addr);
    let synack_frame = Frame::with_data(Command::SynAck, stream_id, Bytes::from(error_msg));
    session.write_control_frame(synack_frame).await?;
}
```

客户端可以更快知道失败原因，而不是长时间等待。

## 超时时间选择依据

| 操作 | 超时时间 | 理由 |
|------|---------|------|
| **TCP 连接** | 15 秒 | DNS (5s) + TCP 握手 (3s) + 缓冲 (7s) |
| **DNS 解析** | 10 秒 | 正常 DNS < 2s，污染/慢速 < 10s |
| **数据转发** | 无 | 使用 TCP KeepAlive，由协议层处理 |

### 为什么不设置更短？

- **3-5 秒太短**：某些地区网络延迟高（300-500ms RTT）
- **30 秒太长**：用户体验差，浏览器会超时
- **15 秒平衡**：
  - 足够处理高延迟网络
  - 快速失败而不是挂起
  - 符合大多数代理软件的默认值

## 其他优化建议

### 1. 使用 debug 日志级别

运行时使用：
```bash
RUST_LOG=warn anytls-server ...
```

或代码中设置：
```bash
anytls-server --log-level warn ...
```

**效果**：
- 减少 90% 日志输出
- 降低 IO 开销
- 提升 5-10% 吞吐量

### 2. DNS 缓存（未实现，可选）

如果需要更高性能，可以考虑：

```rust
use std::collections::HashMap;
use tokio::sync::RwLock;

static DNS_CACHE: Lazy<RwLock<HashMap<String, Vec<SocketAddr>>>> = ...;

async fn resolve_with_cache(domain: &str) -> Result<SocketAddr> {
    // 1. 检查缓存
    if let Some(addrs) = DNS_CACHE.read().await.get(domain) {
        return Ok(addrs[0]);
    }
    
    // 2. 解析并缓存
    let addrs = timeout(DNS_TIMEOUT, lookup_host(domain)).await??;
    DNS_CACHE.write().await.insert(domain.to_string(), addrs);
    Ok(addrs[0])
}
```

**优点**：
- 避免重复 DNS 查询
- YouTube 等热门网站命中率高
- 减少延迟 50-200ms

**缺点**：
- 需要处理 TTL 和缓存失效
- 增加代码复杂度
- 占用内存

### 3. TCP Fast Open（需评估）

对于客户端到服务器的连接可以考虑 TFO：

```toml
# sing-box outbound 配置
"tcp_fast_open": false  # 当前 anytls 不支持，会返回错误
```

**注意**：当前 anytls 协议与 TFO 不兼容（SOCKS 包装需要地址信息）

## 测试验证

### 1. 超时测试

```bash
# 测试 DNS 超时
curl -x socks5://127.0.0.1:1080 http://nonexistent.invalid.domain.test

# 预期：10 秒后返回 DNS 超时错误

# 测试连接超时  
curl -x socks5://127.0.0.1:1080 http://192.0.2.1:12345

# 预期：15 秒后返回连接超时错误
```

### 2. 正常访问测试

```bash
# YouTube
curl -x socks5://127.0.0.1:1080 https://www.youtube.com -I

# Google
curl -x socks5://127.0.0.1:1080 https://www.google.com -I

# 预期：快速返回响应或明确错误
```

### 3. 性能测试

```bash
# 并发测试
ab -n 1000 -c 50 -X 127.0.0.1:1080 http://example.com/

# 观察：
# - 平均响应时间
# - 失败率
# - 超时次数
```

## 部署建议

### 1. 重新编译部署

```bash
# 1. 更新代码
git pull

# 2. 编译
cargo build --release

# 3. 停止旧服务
systemctl stop anytls-server

# 4. 替换二进制
sudo cp target/release/anytls-server /usr/local/bin/

# 5. 启动新服务
systemctl start anytls-server

# 6. 查看日志
journalctl -u anytls-server -f
```

### 2. 监控要点

优化后应该看到：

**成功的日志**：
```log
INFO [Proxy] Successfully connected to youtube.com:443
```

**超时的日志**：
```log
ERROR [Proxy] Connection timeout (15s) to slow-site.example.com:80
```

**不应该再看到**：
- 长时间无日志（说明在阻塞）
- 客户端报 "connection hang"

### 3. 调优参数（可选）

如果 15 秒仍不合适，可以修改：

```rust
// src/server/handler.rs:267
let connect_timeout = Duration::from_secs(15);  // 改为 10 或 20

// src/server/udp_proxy.rs:252  
let dns_timeout = Duration::from_secs(10);      // 改为 5 或 15
```

## 总结

### 问题根源
- **无超时配置**：导致 DNS 和 TCP 连接无限阻塞
- **日志级别过高**：INFO 级别产生大量日志影响性能

### 解决方案
- ✅ TCP 连接 15 秒超时
- ✅ DNS 解析 10 秒超时
- ✅ 超时错误通知客户端
- ⚠️ 日志级别需手动设置为 `warn` 或 `error`

### 预期效果
- YouTube 等网站能正常访问或快速失败
- 不再出现长时间挂起
- 资源利用率提升
- 用户体验改善

### 后续优化方向
1. DNS 缓存（可选）
2. 连接池预热（客户端已有）
3. 智能超时调整（根据历史延迟）
4. 健康检查和熔断机制

---

**版本**：v0.5.3（规划）  
**更新日期**：2025-11-11  
**相关文件**：
- `src/server/handler.rs`
- `src/server/udp_proxy.rs`

