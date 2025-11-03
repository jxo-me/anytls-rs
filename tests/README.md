# 集成测试文档

## 测试结构

本目录包含 AnyTLS-RS 的集成测试，验证端到端功能。

### 测试文件

- **`common.rs`**: 测试工具函数和共享配置
- **`basic_proxy.rs`**: 基本代理功能测试
  - `test_server_startup`: 测试服务器启动
  - `test_client_startup`: 测试客户端启动
  - `test_client_server_connection`: 测试客户端-服务器连接
- **`concurrent.rs`**: 并发连接测试
  - `test_multiple_streams`: 测试多个并发流
  - `test_session_reuse`: 测试会话复用
- **`error_handling.rs`**: 错误处理测试
  - `test_wrong_password`: 测试错误密码处理
  - `test_invalid_server_address`: 测试无效服务器地址处理

## 运行测试

### 运行所有集成测试

```bash
cargo test --test basic_proxy --test concurrent --test error_handling
```

### 运行特定测试文件

```bash
# 基本代理测试
cargo test --test basic_proxy

# 并发测试
cargo test --test concurrent

# 错误处理测试
cargo test --test error_handling
```

### 运行特定测试用例

```bash
cargo test --test basic_proxy test_server_startup
```

## 测试配置

测试使用默认配置：
- 服务器地址: `127.0.0.1:8443`
- 客户端监听: `127.0.0.1:1080`
- 密码: `test_password`

可以通过修改 `common.rs` 中的 `TestConfig::default()` 来更改配置。

## 注意事项

1. 测试会启动真实的服务器和客户端进程，使用实际网络端口
2. 某些测试（如连接外部服务器）可能因网络问题而失败，这是正常的
3. 测试使用随机可用端口可以避免端口冲突（未来改进）

## 测试覆盖

- ✅ 服务器启动和监听
- ✅ 客户端启动和 SOCKS5 服务
- ✅ TLS 连接建立
- ✅ 流创建和会话管理
- ✅ 并发连接处理
- ✅ 错误处理（认证失败、连接失败等）

## 未来改进

- [ ] 使用随机端口避免端口冲突
- [ ] 添加 HTTP 代理测试
- [ ] 添加实际数据传输测试
- [ ] 性能基准测试（使用 criterion）
- [ ] 内存泄漏检测测试

