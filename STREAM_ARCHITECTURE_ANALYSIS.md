# Stream 架构分析与改进建议

## 问题分析

### Go 参考实现的优势

1. **分离的 Pipe 架构**：
   ```go
   type Stream struct {
       pipeR *pipe.PipeReader  // 独立的 reader，无需 mutex
       pipeW *pipe.PipeWriter  // 独立的 writer
   }
   ```

2. **无锁读取**：
   - `Stream.Read()` → `pipeR.Read()` → channel 通信
   - channel 本身是并发安全的，无需 mutex

3. **写入不访问 Stream 状态**：
   - `Stream.Write()` → `sess.writeDataFrame()` 
   - 直接通过 Session 发送，不持有 Stream 的锁

4. **完全并发**：
   - 读取和写入可以同时进行，无锁竞争

### 当前 Rust 实现的问题

1. **整体 Mutex 保护**：
   ```rust
   let stream_mutex = Arc::new(tokio::sync::Mutex::new(stream));
   ```
   - Task1 读取时需要持有 mutex
   - Task2 写入时也需要持有 mutex
   - 导致死锁

2. **reader_rx 在 Stream 内部**：
   - 访问 `reader_rx` 需要获取 mutex
   - 等待 channel 数据时持有锁，阻塞写入

3. **reader_buffer 也需要 mutex**：
   - 缓冲区操作也需要锁保护

## 解决方案

### 方案 1：超时机制（当前实现）

**优点**：
- 实现简单，不需要重构
- 减少锁持有时间

**缺点**：
- 不是根本解决方案
- 仍有潜在的锁竞争

### 方案 2：重构 Stream 架构（推荐）

参考 Go 实现，将 reader 部分从 Stream 的 mutex 中分离：

```rust
pub struct Stream {
    id: u32,
    
    // 读取部分：独立管理，不需要主 mutex
    reader_rx: Arc<Mutex<mpsc::UnboundedReceiver<Bytes>>>,  // 独立的 mutex
    reader_buffer: Arc<Mutex<Vec<u8>>>,  // 独立的 mutex
    
    // 写入部分：通过 Session，不需要 Stream 的 mutex
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    
    // 状态：使用原子类型
    is_closed: Arc<AtomicBool>,
}
```

**Task1 读取**：
- 获取 `reader_rx` 的 mutex（独立的锁）
- 从 channel 读取（无需持有 Stream 的主 mutex）

**Task2 写入**：
- 直接调用 `session.write_data_frame()`（无需 Stream 的 mutex）
- 或者获取 `writer_tx` 的锁（如果需要）

**优点**：
- 彻底解决锁竞争问题
- 与 Go 实现一致

**缺点**：
- 需要重构 Stream 和相关代码
- 修改较多

### 方案 3：直接使用 Session 的 channel

在 `proxy_tcp_connection_data_forwarding` 中：
1. 从 Session 获取 stream 的 channel sender
2. 创建一个新的 channel pair
3. 让 Session 同时发送到两个 channel
4. Task1 从新 channel 读取（无锁）

**优点**：
- Task1 完全不需要 Stream 的 mutex

**缺点**：
- 需要修改 Session 的实现
- 增加了复杂性

## 推荐行动

1. **短期**：测试当前超时方案的效果
2. **中期**：如果问题仍然存在，实施方案 2（重构 Stream）
3. **长期**：考虑完全对齐 Go 实现的架构

## 参考资料

- Go 参考实现：`proxy/session/stream.go`
- Go Pipe 实现：`proxy/pipe/io_pipe.go`
- socks5-impl 库：<https://docs.rs/socks5-impl>

