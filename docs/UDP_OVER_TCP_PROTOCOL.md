# UDP over TCP 协议分析

**协议**: sing-box udp-over-tcp v2  
**参考**: 
- [sing-box 官方文档](https://github.com/SagerNet/sing-box/blob/dev-next/docs/configuration/shared/udp-over-tcp.md)
- Go 实现 `anytls-go/cmd/server/outbound_tcp.go`

---

## 📋 协议概述

### 目标地址

```
sp.v2.udp-over-tcp.arpa
```

当 SOCKS5 目标地址为此特殊域名时，表示使用 UDP over TCP 协议。

---

## 📦 sing-box UDP over TCP v2 协议格式

### 请求格式 (Request)

```
| isConnect | ATYP | Address | Port |
| u8        | u8   | variable| u16be|
```

- **isConnect**: 1 = Connect 格式, 0 = Non-connect 格式
- **ATYP / Address / Port**: SOCKS5 地址格式

### Connect 流格式 (推荐，anytls 使用此格式)

```
| Length | Data     |
| u16be  | variable |
```

**每个 UDP 包**:
- **Length**: UDP 数据长度（Big-Endian uint16）
- **Data**: 实际的 UDP 数据

### Non-connect 流格式

```
| ATYP | Address | Port | Length | Data |
| u8   | variable| u16be| u16be  | variable|
```

**每个 UDP 包都包含地址信息**（不推荐）

---

## 🔍 Go 实现分析

### 服务器端流程

**文件**: `anytls-go/cmd/server/outbound_tcp.go`

```go
func proxyOutboundUoT(ctx context.Context, conn net.Conn, destination M.Socksaddr) error {
    // 1. 读取 UDP over TCP 请求
    request, err := uot.ReadRequest(conn)
    if err != nil {
        return err
    }

    // 2. 创建 UDP socket
    c, err := net.ListenPacket("udp", "")
    if err != nil {
        return err
    }

    // 3. 报告握手成功
    err = N.ReportHandshakeSuccess(conn)
    if err != nil {
        return err
    }

    // 4. 双向数据转发
    return bufio.CopyPacketConn(ctx, uot.NewConn(conn, *request), bufio.NewPacketConn(c))
}
```

### 客户端流程

**文件**: `anytls-go/cmd/client/inbound.go`

```go
func (c *myClient) NewPacketConnection(ctx context.Context, conn network.PacketConn, metadata M.Metadata) error {
    // 1. 创建代理连接，目标为 uot.RequestDestination(2)
    proxyC, err := c.CreateProxy(ctx, uot.RequestDestination(2))
    if err != nil {
        return err
    }
    defer proxyC.Close()

    // 2. 创建 UoT 请求
    request := uot.Request{
        Destination: metadata.Destination,
    }
    
    // 3. 创建 UoT 连接（延迟初始化）
    uotC := uot.NewLazyConn(proxyC, request)

    // 4. 双向数据转发
    return bufio.CopyPacketConn(ctx, conn, uotC)
}
```

---

## ✅ anytls 实施方案

anytls 使用 **Connect 格式** (isConnect=1):

1. **初始请求**: isConnect=1 + SOCKS5 Address
2. **后续数据包**: Length (2 bytes) + Data

**优点**:
- 数据包格式简单
- 无需每次都发送地址信息
- 效率更高

---

## 🎯 Rust 实施计划

### 阶段 2.1: 研究完成 ✅

**发现**:
1. 目标地址: `sp.v2.udp-over-tcp.arpa`
2. 协议格式简单：Length (2字节) + Data
3. 使用 UDP socket 进行实际传输
4. 双向转发：Stream ↔ UDP Socket

---

## 📝 实施要点

### 关键组件

1. **协议检测**: 检查目标域名是否包含 `udp-over-tcp.arpa`
2. **请求解析**: 读取初始请求（如果有）
3. **UDP Socket**: 创建本地 UDP socket
4. **数据封装**: UDP → Length+Data → Stream
5. **数据解封**: Stream → Length+Data → UDP
6. **双向转发**: 持续转发直到连接关闭

### 数据流

```
Client UDP App
     ↓
   Local UDP Socket
     ↓ (封装)
   Stream (AnyTLS)
     ↓ (TLS)
   Server Stream
     ↓ (解封)
   UDP Socket
     ↓
  Target UDP Service
```

---

## 下一步

开始实施 **任务 2.2**: 创建 `udp_proxy.rs` 模块

---

*分析完成时间: 2025-11-03*

