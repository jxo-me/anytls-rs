# AnyTLS 日志配置指南

## 概述

AnyTLS 使用 Rust 的 `tracing` 框架提供结构化日志记录，支持灵活的日志级别控制。

## 日志级别

从高到低的日志级别：

| 级别 | 用途 | 示例 |
|------|------|------|
| **ERROR** | 严重错误，导致功能无法继续 | TLS握手失败、认证失败、连接错误 |
| **WARN** | 警告信息，不影响主要功能 | 会话超时、重试操作、配置问题 |
| **INFO** | 重要的业务事件（默认级别） | 服务启动、新连接、会话创建 |
| **DEBUG** | 详细的调试信息 | 帧发送/接收、流状态变化、内部状态 |
| **TRACE** | 极其详细的追踪信息 | 原始数据、每个步骤的细节 |

## 配置方式

### 1. 命令行参数（推荐）

服务端：
```bash
# 只显示错误
anytls-server -p password -L error

# 显示警告及以上
anytls-server -p password -L warn

# 显示信息及以上（默认）
anytls-server -p password -L info

# 显示调试信息
anytls-server -p password -L debug

# 显示所有追踪信息
anytls-server -p password -L trace
```

客户端：
```bash
# 只显示错误
anytls-client -p password -s server:8443 -L error

# 显示信息（默认）
anytls-client -p password -s server:8443 -L info

# 显示调试信息
anytls-client -p password -s server:8443 -L debug
```

### 2. 环境变量

使用 `RUST_LOG` 环境变量：

```bash
# 全局日志级别
export RUST_LOG=info
anytls-server -p password

# 按模块设置
export RUST_LOG=anytls_rs::session=debug,anytls_rs::server=info
anytls-server -p password

# 只显示特定模块
export RUST_LOG=anytls_rs::client=debug
anytls-client -p password -s server:8443
```

**注意**：环境变量优先级高于命令行参数。

### 3. 混合使用

```bash
# 环境变量未设置时使用命令行参数
anytls-server -p password -L debug

# 环境变量会覆盖命令行参数
RUST_LOG=trace anytls-server -p password -L info  # 实际使用 trace
```

## 不同场景的推荐配置

### 生产环境

```bash
# 服务端 - 只记录重要事件和错误
anytls-server -p password -L info

# 客户端 - 减少日志输出
anytls-client -p password -s server:8443 -L warn
```

**优点**：
- 最小的性能影响
- 日志文件小
- 易于监控重要事件

### 开发调试

```bash
# 显示详细的调试信息
anytls-server -p password -L debug
anytls-client -p password -s server:8443 -L debug
```

**适用于**：
- 功能开发
- 问题排查
- 性能分析

### 深度调试

```bash
# 显示所有追踪信息（仅用于问题诊断）
export RUST_LOG=trace
anytls-server -p password
anytls-client -p password -s server:8443
```

**警告**：
- 会产生大量日志
- 可能影响性能
- 仅用于诊断特定问题

### 按模块细分

```bash
# Session 模块 debug，其他 info
export RUST_LOG=anytls_rs::session=debug,anytls_rs=info
anytls-server -p password

# 只显示错误和 session 的 debug
export RUST_LOG=anytls_rs::session=debug,anytls_rs=error
anytls-server -p password
```

## 日志输出示例

### INFO 级别（默认）

```
anytls-server v0.4.1
Listening on 0.0.0.0:8443
[Server] New connection from 192.168.1.100:54321
[Server] Client authenticated
[Server] Session 1 created
```

**特点**：
- 简洁清晰
- 只显示重要事件
- 适合生产环境

### DEBUG 级别

```
anytls-server v0.4.1
Listening on 0.0.0.0:8443
[Server] New connection from 192.168.1.100:54321
[Server] Starting TLS handshake
[Server] TLS handshake successful
[Server] Authenticating client
[Server] Client authenticated
[Server] Session 1 created
[Server] Starting receive loop
[Server] recv_loop task spawned
[Server] Starting stream data processing
[Server] process_stream_data task spawned
[Session] recv_loop started
[Session] handle_frame: Processing frame cmd=Syn, stream_id=1, data_len=0
[Session] Opening stream 1
```

**特点**：
- 显示详细的操作流程
- 包含内部状态变化
- 适合开发和调试

### TRACE 级别

```
... (包含 DEBUG 的所有内容) ...
[Session] recv_loop: Acquiring reader lock (iteration 1)
[Session] recv_loop: Reader lock acquired, calling read_buf
[Session] recv_loop: read_buf returned 256 bytes
[Session] write_data_frame: stream_id=1, data_len=128
[Client] Writing IPv4 address: [192, 168, 1, 1]
[Client] Writing port: 443
```

**特点**：
- 极其详细
- 包含所有操作细节
- 日志量大，影响性能

## 常见问题

### Q: 如何只看错误日志？

```bash
anytls-server -p password -L error
```

或

```bash
RUST_LOG=error anytls-server -p password
```

### Q: 如何调试特定模块？

```bash
# 只看 session 模块的 debug 日志
RUST_LOG=anytls_rs::session=debug anytls-server -p password

# session 用 trace，其他用 info
RUST_LOG=anytls_rs::session=trace,anytls_rs=info anytls-server -p password
```

### Q: 日志太多，如何减少？

1. 提高日志级别：
```bash
anytls-server -p password -L warn  # 只显示警告和错误
```

2. 重定向到文件：
```bash
anytls-server -p password 2>&1 | tee server.log
```

3. 只记录错误到文件：
```bash
anytls-server -p password 2>> errors.log
```

### Q: 如何在 Docker 中配置日志？

```dockerfile
# Dockerfile
ENV RUST_LOG=info
CMD ["anytls-server", "-p", "password", "-L", "info"]
```

或使用 docker-compose：

```yaml
# docker-compose.yml
services:
  anytls-server:
    image: anytls-rs:latest
    environment:
      - RUST_LOG=debug
    command: ["anytls-server", "-p", "password"]
```

### Q: 生产环境日志配置建议？

**最佳实践**：

```bash
# 1. 使用 info 级别
anytls-server -p password -L info

# 2. 重定向到文件并轮转
anytls-server -p password 2>&1 | rotatelogs -l /var/log/anytls/server.%Y%m%d.log 86400

# 3. 使用 systemd 管理（自动处理日志）
[Service]
ExecStart=/usr/local/bin/anytls-server -p password -L info
StandardOutput=journal
StandardError=journal
```

## 性能影响

| 日志级别 | 性能影响 | 适用场景 |
|---------|---------|---------|
| ERROR | 极小 | 生产环境 |
| WARN | 很小 | 生产环境 |
| INFO | 小 | 生产环境（默认） |
| DEBUG | 中等 | 开发/测试环境 |
| TRACE | 较大 | 问题诊断 |

**建议**：
- 生产环境使用 INFO 或 WARN
- 开发环境使用 DEBUG
- 只在必要时使用 TRACE

## 日志格式

AnyTLS 的日志采用以下格式：

```
[时间] [级别] [模块前缀] 消息内容
```

示例：
```
2025-11-11T12:34:56.789Z  INFO anytls_rs::server: [Server] New connection from 192.168.1.100:54321
2025-11-11T12:34:56.790Z DEBUG anytls_rs::session: [Session] Opening stream 1
2025-11-11T12:34:56.791Z ERROR anytls_rs::client: [Client] Connection failed: timeout
```

## 相关资源

- [日志优化分析报告](./LOG_OPTIMIZATION.md)
- [故障排查指南](./TROUBLESHOOTING.md)
- [Rust Tracing 文档](https://docs.rs/tracing)

