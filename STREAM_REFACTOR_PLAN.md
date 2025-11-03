# Stream 架构重构分析与实现计划

## 一、当前架构分析

### 1.1 当前实现的问题

**核心问题：锁竞争**
- `reader_rx` 和 `reader_buffer` 在 Stream 内部，需要 mutex 保护
- Task1（读取）和 Task2（写入）都需要获取同一个 mutex
- Task1 等待 channel 时持有锁，阻塞 Task2 写入

**当前结构：**
```rust
pub struct Stream {
    id: u32,
    reader_rx: mpsc::UnboundedReceiver<Bytes>,      // 需要 mutex
    reader_buffer: Vec<u8>,                          // 需要 mutex
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>, // 并发安全，但访问需要 mutex
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}
```

**使用场景：**
```rust
// Task1: 读取需要 mutex
let stream_arc = stream_read.lock().await;
AsyncReadExt::read(&mut pinned, &mut buf).await; // 持有锁等待 channel

// Task2: 写入也需要 mutex
let stream_arc = stream_write.lock().await;
AsyncWriteExt::write_all(&mut pinned, &buf).await; // 被 Task1 阻塞
```

### 1.2 Go 参考实现的优势

**分离的设计：**
```go
type Stream struct {
    pipeR *pipe.PipeReader  // 独立的 reader，无锁访问
    pipeW *pipe.PipeWriter  // 独立的 writer
    sess  *Session          // 写入直接通过 Session
}

// Read: 无锁，直接使用 pipeR
func (s *Stream) Read(b []byte) (n int, err error) {
    n, err = s.pipeR.Read(b)  // pipeR.Read 使用 channel，并发安全
    return
}

// Write: 无锁，直接通过 Session
func (s *Stream) Write(b []byte) (n int, err error) {
    n, err = s.sess.writeDataFrame(s.id, b)  // 不需要 Stream 的锁
    return
}
```

**Pipe 实现的核心：**
- `PipeReader.Read()`: 使用 channel (`wrCh`)，**无需 mutex**
- `PipeWriter.Write()`: 使用独立的 mutex (`wrMu`)，**不影响读取**
- 读取和写入完全分离，无锁竞争

## 二、重构方案设计

### 2.1 方案 A：分离 Reader 和 Writer（推荐）

**设计思路：**
将 `reader_rx` 和 `reader_buffer` 提取到独立的 `StreamReader`，`Stream` 只持有引用。

**新架构：**
```rust
// 独立的 Reader，拥有自己的 mutex
pub struct StreamReader {
    reader_rx: mpsc::UnboundedReceiver<Bytes>,
    reader_buffer: Vec<u8>,
    reader_mutex: Arc<tokio::sync::Mutex<()>>,  // 只保护 buffer
}

// Stream 只持有引用
pub struct Stream {
    id: u32,
    reader: Arc<StreamReader>,                    // 共享的 reader
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>, // 写入无需锁
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}
```

**优势：**
- ✅ 读取和写入完全分离
- ✅ Task1 使用 `StreamReader` 的独立锁
- ✅ Task2 写入不需要锁（直接使用 `writer_tx`）
- ✅ 最接近 Go 实现的设计

**缺点：**
- ⚠️ 需要修改所有使用 Stream 的地方
- ⚠️ `AsyncRead` trait 需要重新实现

### 2.2 方案 B：Reader 使用独立的 Mutex

**设计思路：**
保持 Stream 结构，但将 `reader_rx` 和 `reader_buffer` 包装在独立的 `Arc<Mutex<>>` 中。

**新架构：**
```rust
pub struct Stream {
    id: u32,
    // 读取部分：独立的 mutex
    reader: Arc<tokio::sync::Mutex<StreamReaderInner>>,
    
    // 写入部分：并发安全，无需 mutex
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}

struct StreamReaderInner {
    reader_rx: mpsc::UnboundedReceiver<Bytes>,
    reader_buffer: Vec<u8>,
}
```

**优势：**
- ✅ 最小化改动，保持 Stream 的接口
- ✅ 读取有独立的锁，不影响写入

**缺点：**
- ⚠️ 仍然需要 mutex 来访问 reader
- ⚠️ 不如方案 A 彻底

### 2.3 方案 C：使用 tokio::sync::RwLock（不推荐）

**设计思路：**
使用 `RwLock` 替代 `Mutex`，允许多个读取者。

**问题：**
- ❌ `reader_rx` 需要 `&mut`，RwLock 的读锁只能提供 `&`
- ❌ 不适用于需要可变访问的场景

## 三、推荐方案详细设计（方案 A）

### 3.1 新的数据结构

```rust
// 独立的 StreamReader，管理读取状态
pub struct StreamReader {
    id: u32,
    reader_rx: mpsc::UnboundedReceiver<Bytes>,
    reader_buffer: Vec<u8>,
    // 注意：reader_rx.recv() 不需要 mutex（channel 是并发安全的）
    // 但 reader_buffer 需要保护
    buffer_mutex: Arc<tokio::sync::Mutex<()>>, // 轻量级，只保护 buffer
}

impl StreamReader {
    pub async fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        // 1. 快速检查 buffer（需要锁，但时间很短）
        let buffer_data = {
            let _guard = self.buffer_mutex.lock().await;
            if !self.reader_buffer.is_empty() {
                let n = std::cmp::min(self.reader_buffer.len(), buf.len());
                buf[..n].copy_from_slice(&self.reader_buffer[..n]);
                self.reader_buffer.drain(..n);
                return Ok(n);
            }
            None
        };
        
        // 2. Buffer 为空，直接从 channel 读取（无需锁）
        match self.reader_rx.recv().await {
            Some(data) => {
                // 3. 处理数据（可能需要再次获取锁来更新 buffer）
                self.process_received_data(data, buf)
            }
            None => Ok(0) // EOF
        }
    }
    
    pub fn push_data(&self, data: Bytes) {
        // Session 调用此方法推送数据
        // 这个调用本身不需要锁（channel 是并发安全的）
        // 但需要在收到数据后更新 buffer（需要锁）
    }
}

// Stream 持有 reader 的引用
pub struct Stream {
    id: u32,
    reader: Arc<StreamReader>,
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}
```

### 3.2 AsyncRead 实现

```rust
impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // 直接调用 StreamReader，无需 Stream 的锁
        self.reader.poll_read(cx, buf)
    }
}
```

### 3.3 AsyncWrite 实现（无需改动）

```rust
impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // 直接使用 writer_tx，无需锁
        match self.writer_tx.send((self.id, Bytes::copy_from_slice(buf))) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "session channel closed",
            ))),
        }
    }
}
```

### 3.4 Session 的修改

```rust
impl Session {
    // 创建 Stream 时返回 reader 的引用
    pub async fn open_stream(&self) -> Result<(Arc<Stream>, Arc<StreamReader>)> {
        let (receive_tx, receive_rx) = mpsc::unbounded_channel();
        
        let reader = Arc::new(StreamReader::new(stream_id, receive_rx));
        let stream = Arc::new(Stream::new(
            stream_id,
            Arc::clone(&reader),
            self.stream_data_tx.clone(),
        ));
        
        // 存储 reader 的 sender（用于推送数据）
        self.stream_receive_tx.write().await.insert(stream_id, receive_tx);
        
        Ok((stream, reader))
    }
    
    // handle_frame 推送数据到 reader
    async fn handle_frame(&self, frame: Frame) -> Result<()> {
        match frame.cmd {
            Command::Push => {
                // 获取 reader（或通过 stream_receive_tx）
                if let Some(reader) = self.get_stream_reader(frame.stream_id).await {
                    reader.push_data(frame.data);
                }
            }
            // ...
        }
    }
}
```

## 四、优缺点分析

### 4.1 方案 A（分离 Reader/Writer）的优缺点

**优点：**
1. ✅ **彻底解决锁竞争**：读取和写入完全分离
2. ✅ **性能最优**：无需超时机制，无锁等待
3. ✅ **对齐 Go 实现**：架构一致，易于维护
4. ✅ **代码更清晰**：职责分离，可读性更好
5. ✅ **扩展性好**：未来添加功能更容易

**缺点：**
1. ❌ **改动范围大**：需要修改 Stream、Session、handler 等多个文件
2. ❌ **API 变更**：`AsyncRead` 的实现需要调整
3. ❌ **测试成本高**：需要全面测试所有场景
4. ❌ **迁移风险**：可能引入新的 bug

### 4.2 方案 B（独立 Mutex）的优缺点

**优点：**
1. ✅ **改动较小**：保持现有 API，最小化修改
2. ✅ **风险较低**：改动范围可控
3. ✅ **向后兼容**：不需要大范围修改调用代码

**缺点：**
1. ❌ **不完全解决问题**：仍然需要 mutex 来访问 reader
2. ❌ **性能提升有限**：比超时方案好，但不如方案 A
3. ❌ **架构不够优雅**：仍然是妥协方案

### 4.3 与当前超时方案对比

**当前超时方案：**
- ✅ **已实现且工作正常**：问题已解决
- ✅ **零风险**：不需要重构
- ❌ **不是最优解**：仍有潜在的锁竞争（虽然被缓解）
- ❌ **增加了复杂度**：超时逻辑需要维护

**重构方案 A：**
- ✅ **根本解决**：完全消除锁竞争
- ✅ **性能最优**：无需超时等待
- ❌ **需要重构**：改动大，风险高

## 五、实现计划

### 5.1 阶段一：准备阶段（1-2 天）

**目标：** 充分理解当前实现，设计详细的重构方案

**任务：**
1. ✅ 分析所有使用 Stream 的地方
2. ✅ 设计新的 StreamReader API
3. ✅ 设计 Session 与 StreamReader 的交互方式
4. ✅ 编写详细的接口文档

**输出：**
- StreamReader 的完整 API 设计
- 迁移路径文档
- 测试计划

### 5.2 阶段二：核心重构（3-5 天）

**步骤 1：创建 StreamReader（1 天）**
- 创建 `src/session/stream_reader.rs`
- 实现 `StreamReader` 结构
- 实现 `read()` 和 `push_data()` 方法
- 单元测试

**步骤 2：重构 Stream（1 天）**
- 修改 `Stream` 结构，持有 `Arc<StreamReader>`
- 更新 `AsyncRead` 实现
- 保持 `AsyncWrite` 不变
- 更新相关方法

**步骤 3：更新 Session（1 天）**
- 修改 `open_stream()` 返回 `(Stream, StreamReader)`
- 更新 `handle_frame()` 使用 `StreamReader.push_data()`
- 更新 Stream 存储方式

**步骤 4：更新 Handler（1-2 天）**
- 修改 `proxy_tcp_connection_data_forwarding` 使用新的 API
- 修改 `read_socks_addr` 使用新的 API
- 更新客户端 `socks5.rs` 中的使用
- 移除超时逻辑（不再需要）

### 5.3 阶段三：测试与优化（2-3 天）

**任务：**
1. 运行所有现有测试
2. 添加新的单元测试
3. 集成测试（多并发场景）
4. 性能测试（对比超时方案）
5. 修复发现的问题

### 5.4 阶段四：清理（1 天）

**任务：**
1. 移除超时相关代码
2. 清理临时方法和注释
3. 更新文档
4. 代码审查

**总计时间估算：** 7-11 天

## 六、风险评估与缓解

### 6.1 技术风险

**风险 1：引入新的并发 bug**
- **概率：** 中
- **影响：** 高
- **缓解：**
  - 详细的代码审查
  - 充分的单元测试和集成测试
  - 使用 `Arc` 和 `Mutex` 的最佳实践

**风险 2：性能回退**
- **概率：** 低
- **影响：** 中
- **缓解：**
  - 性能基准测试
  - 对比重构前后的性能指标
  - 如有问题，可以回滚

**风险 3：API 不兼容**
- **概率：** 中
- **影响：** 中
- **缓解：**
  - 保持 `AsyncRead` 和 `AsyncWrite` trait 的兼容性
  - 内部实现变更，外部 API 不变

### 6.2 时间风险

**风险：** 实际开发时间超出预期
- **缓解：**
  - 分阶段实施，每个阶段可独立测试
  - 保留当前实现作为回滚方案
  - 优先保证核心功能，细节可后续优化

### 6.3 回滚方案

**如果重构失败：**
1. 保留当前超时方案（已经工作正常）
2. 使用 Git 分支，方便回滚
3. 在合并前进行全面测试

## 七、决策建议

### 7.1 推荐策略

**短期（当前）：**
- ✅ **保持当前超时方案**
- ✅ 系统已经工作正常，无需立即重构
- ✅ 继续收集性能数据和用户反馈

**中期（1-2 个月后）：**
- 🔄 **评估是否需要重构**
- 如果性能成为瓶颈，或需要更好的并发性能
- 如果团队有足够时间进行重构
- 考虑实施方案 A（分离 Reader/Writer）

**长期（根据实际需求）：**
- 📈 如果系统稳定且性能满足需求，可能不需要重构
- 📈 如果扩展新功能时遇到架构限制，再进行重构

### 7.2 重构触发条件

**建议在以下情况下考虑重构：**
1. ✅ 性能测试显示超时方案成为瓶颈
2. ✅ 需要支持更高的并发连接数
3. ✅ 添加新功能时发现架构限制
4. ✅ 团队有充足的开发和测试时间
5. ✅ 有明确的性能优化目标

### 7.3 不推荐重构的情况

**不建议重构的情况：**
1. ❌ 当前方案工作正常，性能满足需求
2. ❌ 时间紧迫，有其他更重要的工作
3. ❌ 团队对 Rust 并发模型不够熟悉
4. ❌ 缺少充分的测试覆盖

## 八、实施检查清单

如果决定重构，使用以下检查清单：

### 重构前
- [ ] 完成所有当前功能的测试
- [ ] 建立性能基准线
- [ ] 创建详细的重构计划
- [ ] 获得团队批准
- [ ] 创建功能分支

### 重构中
- [ ] 每个步骤后运行测试
- [ ] 代码审查每个 PR
- [ ] 保持与主分支同步
- [ ] 记录所有设计决策

### 重构后
- [ ] 所有测试通过
- [ ] 性能测试对比
- [ ] 集成测试通过
- [ ] 代码审查完成
- [ ] 文档更新
- [ ] 回滚方案准备就绪

## 九、总结

**当前状态：** ✅ 系统工作正常，超时方案有效

**重构价值：** 
- 架构更优雅，性能更优
- 但需要投入时间和承担风险

**建议：**
1. **当前阶段**：继续使用超时方案，关注性能和稳定性
2. **未来评估**：根据实际需求和性能指标，决定是否需要重构
3. **如果重构**：采用方案 A（分离 Reader/Writer），参考 Go 实现

**关键原则：**
- 🎯 "如果它没坏，就别修它" - 当前方案已工作正常
- 🎯 "优化热点，而不是预优化" - 等待性能数据
- 🎯 "分阶段实施，降低风险" - 如果决定重构，逐步进行

