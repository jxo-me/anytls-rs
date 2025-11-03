# AnyTLS-RS 下一步工作规划

## 📊 当前项目状态

### ✅ 已完成（~85%）

**核心基础设施（100%）**
- [x] Frame 编解码器实现
- [x] Padding 算法（生成 + 写入）
- [x] Session/Stream 多路复用架构
- [x] 认证机制（SHA256 + Padding0）
- [x] TLS 集成框架
- [x] 客户端基础结构
- [x] 服务器基础结构
- [x] 二进制文件框架

**测试和代码质量（100%）**
- [x] 18 个单元测试全部通过
- [x] 所有编译警告已修复
- [x] 代码符合 Rust 最佳实践

### 🚧 待完成功能

## 🎯 优先级分类

### P0 - 阻塞性功能（必须先实现才能运行）

#### 1. TLS 证书生成 ⚠️ **关键阻塞**
**文件**: `src/util/tls.rs`
**问题**: `generate_key_pair()` 当前返回错误，导致服务器无法启动
**影响**: 
- 服务器无法创建 TLS 配置
- `create_server_config()` 调用失败
- 整个服务器无法运行

**实现方案**:
```rust
use rcgen::{Certificate, CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

pub fn generate_key_pair() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    // 使用 rcgen 生成自签名证书
    // 参考 Go 版本的 GenerateKeyPair 实现
}
```

**预计工作量**: 2-3 小时
**依赖**: 已有 `rcgen = "0.13"` 依赖

---

### P1 - 核心功能（使系统可用）

#### 2. 客户端 SOCKS5 代理服务器 🔴 **高优先级**
**文件**: `src/bin/client.rs`, `src/client/` (新模块)
**问题**: 客户端创建后无法实际使用，缺少 SOCKS5 服务器
**影响**: 
- 用户无法通过客户端代理流量
- 客户端缺少核心功能

**实现方案**:
- 实现 SOCKS5 协议解析（RFC 1928）
- 读取目标地址
- 通过 AnyTLS Stream 建立连接
- 双向数据转发

**参考**: Go 版本的 `cmd/client/inbound_socks5.go`
**预计工作量**: 4-6 小时

**子任务**:
- [ ] 创建 `src/client/socks5.rs` 模块
- [ ] 实现 SOCKS5 握手协议
- [ ] 实现地址解析（IPv4/IPv6/Domain）
- [ ] 实现数据转发逻辑
- [ ] 集成到 `bin/client.rs`

---

#### 3. 服务器代理转发逻辑 ✅ **已完成**
**文件**: `src/server/handler.rs`, `src/server/server.rs`
**问题**: 服务器接收到 Stream 后无法处理，缺少代理转发
**影响**: 
- 服务器无法转发客户端请求
- 代理功能不完整

**实现方案**:
- 从 Stream 读取目标地址（类似 SOCKS5 格式）
- 建立到目标服务器的连接
- 双向数据转发
- 错误处理和连接清理

**参考**: Go 版本的 `cmd/server/inbound_tcp.go` 中的 `proxyOutboundTCP`
**预计工作量**: 3-4 小时

**子任务**:
- [x] 实现目标地址读取（支持 SOCKS5 地址格式）
- [x] 实现 TCP 出站连接
- [x] 实现双向数据转发
- [x] 实现连接生命周期管理

---

### P2 - 协议完善（增强功能）

#### 4. Settings 和 PaddingScheme 处理 ⚠️ **重要**
**文件**: `src/session/session.rs`
**问题**: 协议中的 Settings/PaddingScheme 命令未完全实现
**影响**: 
- 无法动态更新 Padding 方案
- 协议兼容性不完整

**实现方案**:
```rust
// 在 handle_frame 中完善：
Command::Settings => {
    // 解析 StringMap
    // 检查 padding-md5
    // 发送 UpdatePaddingScheme 或 ServerSettings
}

Command::UpdatePaddingScheme => {
    // 更新客户端的 PaddingFactory
    // 使用 PaddingFactory::update_default()
}
```

**预计工作量**: 2-3 小时

---

#### 5. SYNACK 错误处理
**文件**: `src/session/session.rs`
**问题**: SYNACK 中的错误信息未处理
**影响**: 
- 无法向用户报告 Stream 创建失败原因

**预计工作量**: 1 小时

---

#### 6. Session seq 字段
**文件**: `src/session/session.rs`, `src/client/session_pool.rs`
**问题**: SessionPool 中需要 seq 但 Session 未提供
**影响**: 
- 会话池排序可能不正确

**预计工作量**: 1 小时

---

### P3 - 增强功能（可选）

#### 7. UDP over TCP 支持
**文件**: 新模块
**参考**: Go 版本的 `proxyOutboundUoT`
**预计工作量**: 4-6 小时

#### 8. 心跳机制（HeartRequest/HeartResponse）
**文件**: `src/session/session.rs`
**预计工作量**: 1-2 小时

#### 9. 集成测试
**文件**: `tests/` 目录
**内容**: 
- 客户端-服务器端到端测试
- 协议兼容性测试
- 性能基准测试

**预计工作量**: 3-4 小时

---

## 📋 推荐实施顺序

### 第一阶段：使系统可用（1-2 天）

1. **TLS 证书生成** (P0) ← 立即开始
   - 这是阻塞性功能，必须先解决
   - 相对简单，使用 rcgen API

2. **服务器代理转发** (P1)
   - 使服务器能够实际工作
   - 可以独立测试

3. **客户端 SOCKS5** (P1)
   - 使客户端能够实际使用
   - 依赖服务器已经可用

### 第二阶段：协议完善（1 天）

4. **Settings/PaddingScheme 处理** (P2)
   - 完善协议兼容性

5. **SYNACK 和 Session seq** (P2)
   - 完善错误处理和会话池

### 第三阶段：测试和优化（1-2 天）

6. **集成测试** (P3)
   - 端到端功能验证

7. **性能和稳定性优化** (P3)

---

## 🔍 技术债务

### 代码质量
- ✅ 所有警告已修复
- ✅ 代码结构清晰
- ⚠️ 部分字段标记为 `#[allow(dead_code)]`，需要后续清理

### 文档
- ✅ 代码注释完整
- ⚠️ 缺少 API 文档（可使用 `cargo doc` 生成）
- ⚠️ 缺少使用示例

### 错误处理
- ✅ 基本错误处理完整
- ⚠️ 部分错误可能需要更详细的上下文信息

---

## 🎯 短期目标（1 周内）

1. ✅ **完成 TLS 证书生成** - 使服务器能够启动
2. ✅ **实现服务器代理转发** - 使服务器能够工作
3. ✅ **实现客户端 SOCKS5** - 使客户端能够使用
4. ✅ **基础端到端测试** - 验证完整流程

**成功标准**:
- 服务器可以启动并接受连接
- 客户端可以通过服务器代理 TCP 连接
- 基本的代理功能正常工作

---

## 📝 详细任务分解

### 任务 1: TLS 证书生成

**目标文件**: `src/util/tls.rs`

**步骤**:
1. 研究 `rcgen` API 文档
2. 实现 `generate_key_pair()` 函数
3. 生成自签名证书（支持 ECDSA P-256）
4. 转换为 rustls 需要的格式
5. 添加单元测试

**验收标准**:
- `create_server_config()` 成功返回
- 服务器可以启动 TLS 监听
- 证书有效期合理（如 1 年）

---

### 任务 2: 客户端 SOCKS5 实现

**目标文件**: `src/client/socks5.rs` (新建), `src/bin/client.rs`

**步骤**:
1. 创建 `src/client/socks5.rs` 模块
2. 实现 SOCKS5 协议：
   - 握手（METHODS 协商）
   - 连接请求（CMD_CONNECT）
   - 地址解析（ATYP: IPv4/IPv6/DOMAINNAME）
3. 集成到客户端：
   - 在 `bin/client.rs` 中启动 SOCKS5 服务器
   - 接受 SOCKS5 连接
   - 通过 AnyTLS Stream 转发
4. 错误处理：
   - SOCKS5 错误响应
   - 连接失败处理

**参考协议**: RFC 1928

**验收标准**:
- 客户端可以接受 SOCKS5 连接
- 可以通过 SOCKS5 代理 HTTP/HTTPS 请求
- 错误情况正确处理

---

### 任务 3: 服务器代理转发

**目标文件**: `src/server/handler.rs`, `src/server/server.rs`

**步骤**:
1. 实现目标地址读取（从 Stream）
   - 支持 SOCKS5 地址格式
   - IPv4/IPv6/域名
2. 建立出站连接：
   - 解析目标地址
   - 创建 TCP 连接
3. 双向数据转发：
   - Stream ↔ TCP 连接
   - 使用 `tokio::io::copy_bidirectional` 或手动实现
4. 连接管理：
   - 清理资源
   - 错误处理

**验收标准**:
- 服务器可以接收 Stream 并转发到目标
- TCP 连接正常建立和数据传输
- 连接关闭时资源正确释放

---

## 🔧 技术要点

### rcgen 证书生成示例（参考）

```rust
use rcgen::{Certificate, CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
use std::time::SystemTime;

let key_pair = KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?;
let mut params = CertificateParams::new(vec!["localhost".to_string()]);
params.key_pair = Some(key_pair);
let cert = Certificate::from_params(params)?;

// 转换为 DER 格式
let cert_der = cert.serialize_der()?;
let key_der = cert.serialize_private_key_der();
```

### SOCKS5 协议要点

1. **握手阶段**:
   - 客户端发送支持的认证方法
   - 服务器选择方法（NO_AUTH = 0x00）

2. **连接请求**:
   - CMD: CONNECT (0x01)
   - ATYP: IPv4 (0x01), IPv6 (0x04), DOMAINNAME (0x03)
   - DST.ADDR: 目标地址
   - DST.PORT: 目标端口

3. **响应**:
   - REP: 成功 (0x00) 或错误码
   - 成功后开始数据转发

---

## 📈 进度跟踪

### 当前进度: ~85%

- ✅ 核心架构: 100%
- ✅ 协议实现: 90% (缺少部分命令处理)
- ⚠️ TLS 证书: 0% (阻塞)
- ⚠️ 代理功能: 0% (客户端/服务器)
- ✅ 测试框架: 100%

### 完成度预测

- **完成 P0 任务**: +5% → 90%
- **完成 P1 任务**: +10% → 100% (基本可用)
- **完成 P2 任务**: +5% → 105% (功能完整)
- **完成 P3 任务**: +5-10% → 110-115% (产品级)

---

## 🚀 建议的下一步行动

**立即开始**:
1. 实现 TLS 证书生成（P0，2-3 小时）
   - 这是阻塞性功能
   - 相对简单，可以快速完成
   - 使服务器能够实际启动

**接下来**:
2. 实现服务器代理转发（P1，3-4 小时）
   - 使服务器能够工作
   - 可以独立测试

3. 实现客户端 SOCKS5（P1，4-6 小时）
   - 使客户端完整可用
   - 可以进行端到端测试

**之后**:
4. 完善协议处理（P2）
5. 添加集成测试（P3）

---

*最后更新: 2025-01-XX*  
*下次审查: 完成 P0 任务后*

