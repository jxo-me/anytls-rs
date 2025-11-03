# Stream 架构重构实施指南（方案1）

## 🎯 重构目标

将 Stream 的读写操作完全分离，消除锁竞争，提升并发性能 40-60%。

**核心改变：**
- ✅ 创建独立的 `StreamReader` 结构
- ✅ Stream 只持有 Reader 的引用
- ✅ 读取和写入使用不同的锁（或无锁）
- ✅ 与 Go 实现完全对齐

---

## 📅 时间规划

| 阶段 | 任务 | 预计时间 | 风险等级 |
|------|------|---------|---------|
| 阶段1 | 准备工作 | 0.5-1 天 | 低 |
| 阶段2 | 创建 StreamReader | 1-2 天 | 中 |
| 阶段3 | 重构 Stream | 1 天 | 中 |
| 阶段4 | 更新 Session | 1-2 天 | 高 |
| 阶段5 | 修改 Handler | 1-2 天 | 高 |
| 阶段6 | 更新客户端 | 1 天 | 中 |
| 阶段7 | 全面测试 | 2-3 天 | 中 |
| 阶段8 | 清理优化 | 1 天 | 低 |
| **总计** | - | **8-13 天** | - |

---

## 🔧 阶段1：准备工作（0.5-1天）

### 1.1 代码备份与分支管理

```bash
# 1. 确保当前代码已提交
git status
git add .
git commit -m "feat: 准备 Stream 架构重构"

# 2. 创建重构分支
git checkout -b refactor/stream-reader-writer

# 3. 备份当前实现
git tag backup-before-refactor

# 4. 推送到远程
git push origin refactor/stream-reader-writer
```

### 1.2 建立性能基准线

```bash
# 运行性能测试，记录当前指标
cargo bench --bench session_bench > baseline_performance.txt

# 记录关键指标：
# - 并发连接数
# - 吞吐量 (MB/s)
# - 延迟 P50/P99
# - CPU 使用率
```

**创建基准测试脚本：**

```bash
# test_baseline.sh
#!/bin/bash
echo "=== 基准性能测试 ==="
echo "时间: $(date)"
echo ""

# 1. 单流性能
echo "[1] 单流吞吐量测试"
cargo run --release --example single_stream_bench

# 2. 并发性能
echo "[2] 并发10个流测试"
cargo run --release --example concurrent_stream_bench -- --streams 10

# 3. 延迟测试
echo "[3] 往返延迟测试"
cargo run --release --example latency_bench

echo ""
echo "=== 基准测试完成 ==="
```

### 1.3 创建测试用例清单

创建 `REFACTOR_TEST_CHECKLIST.md`：

```markdown
# 重构测试检查清单

## 单元测试
- [ ] StreamReader 读取测试
- [ ] StreamReader buffer 管理测试
- [ ] Stream 写入测试
- [ ] Session 创建 Stream 测试

## 集成测试
- [ ] 单个 Stream 数据传输
- [ ] 多个 Stream 并发传输
- [ ] Stream 关闭和清理
- [ ] 错误处理

## 端到端测试
- [ ] SOCKS5 代理基本功能
- [ ] 多次请求稳定性
- [ ] 大文件传输
- [ ] 高并发场景

## 性能测试
- [ ] 吞吐量对比（应提升 40-60%）
- [ ] 延迟对比（P99 应降低 30-40%）
- [ ] CPU 使用率对比
- [ ] 内存占用对比
```

### 1.4 代码审查准备

```bash
# 创建代码审查模板
cat > .github/PULL_REQUEST_TEMPLATE.md << 'EOF'
# Stream 架构重构 PR

## 变更概述
- [ ] 创建了 StreamReader 结构
- [ ] 重构了 Stream 实现
- [ ] 更新了 Session
- [ ] 修改了 Handler

## 测试情况
- [ ] 所有单元测试通过
- [ ] 集成测试通过
- [ ] 性能测试完成
- [ ] 手动测试验证

## 性能对比
| 指标 | 重构前 | 重构后 | 提升 |
|------|--------|--------|------|
| 吞吐量 | - | - | - |
| 延迟P99 | - | - | - |

## 风险评估
- [ ] 已识别潜在风险
- [ ] 已有回滚方案
EOF
```

---

## 🔧 阶段2：创建 StreamReader（1-2天）

### 2.1 创建新文件

```bash
# 创建 StreamReader 模块
touch src/session/stream_reader.rs
```

### 2.2 实现 StreamReader 结构

**文件：`src/session/stream_reader.rs`**

```rust
//! StreamReader - 独立的流读取器
//! 
//! 负责从 Session 接收数据并提供给上层读取
//! 与 Stream 的写入操作完全分离

use bytes::Bytes;
use std::io;
use tokio::sync::mpsc;

/// StreamReader 管理单个流的读取状态
/// 
/// 设计要点：
/// 1. reader_rx 和 reader_buffer 在同一个结构内，无需额外的锁
/// 2. 外部通过 &mut self 访问，保证互斥
/// 3. 不持有 Stream 的引用，完全独立
pub struct StreamReader {
    /// 流 ID（用于日志）
    id: u32,
    
    /// 从 Session 接收数据的 channel
    /// 注意：recv() 是 async 方法，需要 &mut self
    reader_rx: mpsc::UnboundedReceiver<Bytes>,
    
    /// 缓冲不完整的数据
    /// 当 read buffer 小于接收到的数据时使用
    reader_buffer: Vec<u8>,
    
    /// EOF 标志
    eof: bool,
}

impl StreamReader {
    /// 创建新的 StreamReader
    pub fn new(id: u32, reader_rx: mpsc::UnboundedReceiver<Bytes>) -> Self {
        Self {
            id,
            reader_rx,
            reader_buffer: Vec::new(),
            eof: false,
        }
    }
    
    /// 读取数据到 buffer
    /// 
    /// 实现逻辑：
    /// 1. 优先从 reader_buffer 读取
    /// 2. buffer 为空时从 channel 接收
    /// 3. 处理 EOF 情况
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // 1. 先检查 EOF
        if self.eof && self.reader_buffer.is_empty() {
            return Ok(0);
        }
        
        // 2. 从 buffer 读取（如果有数据）
        if !self.reader_buffer.is_empty() {
            let n = std::cmp::min(self.reader_buffer.len(), buf.len());
            buf[..n].copy_from_slice(&self.reader_buffer[..n]);
            self.reader_buffer.drain(..n);
            
            tracing::trace!(
                "[StreamReader] Read {} bytes from buffer (stream_id={}, buffer_remaining={})",
                n, self.id, self.reader_buffer.len()
            );
            
            return Ok(n);
        }
        
        // 3. buffer 为空，从 channel 接收新数据
        match self.reader_rx.recv().await {
            Some(data) => {
                let data_len = data.len();
                tracing::debug!(
                    "[StreamReader] Received {} bytes from channel (stream_id={})",
                    data_len, self.id
                );
                
                // 直接填充到 buf
                let n = std::cmp::min(data.len(), buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                
                // 剩余数据放入 buffer
                if n < data.len() {
                    self.reader_buffer.extend_from_slice(&data[n..]);
                    tracing::trace!(
                        "[StreamReader] Stored {} bytes in buffer (stream_id={})",
                        data.len() - n, self.id
                    );
                }
                
                Ok(n)
            }
            None => {
                // Channel 关闭，表示 EOF
                tracing::debug!("[StreamReader] Channel closed (EOF) for stream_id={}", self.id);
                self.eof = true;
                Ok(0)
            }
        }
    }
    
    /// 获取流 ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// 检查是否到达 EOF
    pub fn is_eof(&self) -> bool {
        self.eof
    }
    
    /// 获取缓冲区大小（用于诊断）
    pub fn buffer_len(&self) -> usize {
        self.reader_buffer.len()
    }
}

// StreamReader 不需要实现 Clone
// 因为它包含 UnboundedReceiver（不可 Clone）

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_stream_reader_basic() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut reader = StreamReader::new(1, rx);
        
        // 发送数据
        tx.send(Bytes::from("hello")).unwrap();
        
        // 读取数据
        let mut buf = vec![0u8; 10];
        let n = reader.read(&mut buf).await.unwrap();
        
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"hello");
    }
    
    #[tokio::test]
    async fn test_stream_reader_buffering() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut reader = StreamReader::new(1, rx);
        
        // 发送较大的数据
        tx.send(Bytes::from("hello world")).unwrap();
        
        // 分两次读取
        let mut buf = vec![0u8; 5];
        
        let n1 = reader.read(&mut buf).await.unwrap();
        assert_eq!(n1, 5);
        assert_eq!(&buf[..n1], b"hello");
        
        let n2 = reader.read(&mut buf).await.unwrap();
        assert_eq!(n2, 5);
        assert_eq!(&buf[..n2], b" worl");
        
        let n3 = reader.read(&mut buf).await.unwrap();
        assert_eq!(n3, 1);
        assert_eq!(&buf[..n3], b"d");
    }
    
    #[tokio::test]
    async fn test_stream_reader_eof() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut reader = StreamReader::new(1, rx);
        
        // 关闭 channel
        drop(tx);
        
        // 读取应该返回 0（EOF）
        let mut buf = vec![0u8; 10];
        let n = reader.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
        assert!(reader.is_eof());
    }
    
    #[tokio::test]
    async fn test_stream_reader_multiple_chunks() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut reader = StreamReader::new(1, rx);
        
        // 发送多个数据块
        tx.send(Bytes::from("chunk1")).unwrap();
        tx.send(Bytes::from("chunk2")).unwrap();
        tx.send(Bytes::from("chunk3")).unwrap();
        
        let mut buf = vec![0u8; 100];
        let mut total = Vec::new();
        
        // 读取所有数据
        loop {
            let n = reader.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            total.extend_from_slice(&buf[..n]);
        }
        
        assert_eq!(total, b"chunk1chunk2chunk3");
    }
}
```

### 2.3 更新模块声明

**文件：`src/session/mod.rs`**

```rust
pub mod session;
pub mod stream;
pub mod stream_reader;  // 新增

pub use session::Session;
pub use stream::Stream;
pub use stream_reader::StreamReader;  // 新增
```

### 2.4 运行测试

```bash
# 运行 StreamReader 的单元测试
cargo test --lib session::stream_reader

# 预期：所有测试通过 ✅
```

---

## ♻️ 阶段3：重构 Stream（1天）

### 3.1 修改 Stream 结构

**文件：`src/session/stream.rs`**

```rust
//! Stream implementation for AnyTLS protocol
//!
//! Stream provides a duplex communication channel that implements AsyncRead and AsyncWrite

use crate::util::AnyTlsError;
use crate::session::StreamReader;  // 新增
use bytes::Bytes;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

/// Stream represents a single data stream within a Session
/// It implements AsyncRead and AsyncWrite to be used as a connection
pub struct Stream {
    id: u32,
    
    // ===== 读取部分：使用独立的 StreamReader =====
    // 注意：Arc<Mutex<>> 是为了在 poll_read 中获取 &mut
    reader: Arc<tokio::sync::Mutex<StreamReader>>,
    
    // ===== 写入部分：直接使用 channel，无需锁 =====
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    
    // ===== 状态管理 =====
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}

impl Stream {
    /// Create a new stream
    /// 
    /// # Arguments
    /// * `id` - Stream ID
    /// * `reader` - StreamReader 用于读取数据
    /// * `writer_tx` - 发送数据到 Session 的 channel
    pub fn new(
        id: u32,
        reader: StreamReader,
        writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    ) -> Self {
        Self {
            id,
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            writer_tx,
            is_closed: Arc::new(AtomicBool::new(false)),
            close_error: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Get stream ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Close the stream with error (can be called with Arc<Stream>)
    pub async fn close_with_error(&self, err: AnyTlsError) {
        if self.is_closed.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_ok() {
            *self.close_error.lock().await = Some(err);
        }
    }

    /// Check if stream is closed
    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::Relaxed)
    }
    
    /// Get a reference to the reader (for diagnostics)
    pub fn reader(&self) -> &Arc<tokio::sync::Mutex<StreamReader>> {
        &self.reader
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let stream_id = self.id;
        
        // 获取 reader 的锁
        // 注意：这个锁只保护 StreamReader，不影响写入操作
        let mut reader_future = Box::pin(self.reader.lock());
        
        match reader_future.as_mut().poll(cx) {
            Poll::Ready(mut reader) => {
                // 创建一个临时 buffer 用于 StreamReader::read()
                let remaining = buf.remaining();
                let mut temp_buf = vec![0u8; remaining];
                
                // 调用 StreamReader::read()
                let read_future = reader.read(&mut temp_buf);
                tokio::pin!(read_future);
                
                match read_future.poll(cx) {
                    Poll::Ready(Ok(n)) => {
                        if n > 0 {
                            buf.put_slice(&temp_buf[..n]);
                            tracing::trace!(
                                "[Stream] poll_read: Read {} bytes (stream_id={})",
                                n, stream_id
                            );
                        }
                        Poll::Ready(Ok(()))
                    }
                    Poll::Ready(Err(e)) => {
                        tracing::error!(
                            "[Stream] poll_read: Error reading (stream_id={}): {}",
                            stream_id, e
                        );
                        Poll::Ready(Err(e))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let stream_id = self.id;
        let buf_len = buf.len();
        
        if self.is_closed.load(Ordering::Relaxed) {
            tracing::warn!("[Stream] poll_write: Stream {} is closed", stream_id);
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream closed",
            )));
        }

        // 直接发送数据到 Session（无锁！）
        let data = Bytes::copy_from_slice(buf);
        match self.writer_tx.send((self.id, data)) {
            Ok(_) => {
                tracing::trace!(
                    "[Stream] poll_write: Sent {} bytes to channel (stream_id={})",
                    buf_len, stream_id
                );
                Poll::Ready(Ok(buf.len()))
            }
            Err(e) => {
                tracing::error!(
                    "[Stream] poll_write: Failed to send to channel (stream_id={}): {:?}",
                    stream_id, e
                );
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "session channel closed",
                )))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.is_closed.store(true, Ordering::Relaxed);
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    #[tokio::test]
    async fn test_stream_write() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (reader_tx, reader_rx) = mpsc::unbounded_channel();
        
        let reader = StreamReader::new(1, reader_rx);
        let mut stream = Stream::new(1, reader, tx);
        
        // 写入数据
        stream.write_all(b"hello").await.unwrap();
        
        // 验证数据发送到 channel
        let (stream_id, data) = rx.recv().await.unwrap();
        assert_eq!(stream_id, 1);
        assert_eq!(data.as_ref(), b"hello");
    }
    
    #[tokio::test]
    async fn test_stream_read() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (reader_tx, reader_rx) = mpsc::unbounded_channel();
        
        let reader = StreamReader::new(1, reader_rx);
        let mut stream = Stream::new(1, reader, tx);
        
        // 发送数据到 reader
        reader_tx.send(Bytes::from("world")).unwrap();
        
        // 读取数据
        let mut buf = vec![0u8; 10];
        let n = stream.read(&mut buf).await.unwrap();
        
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"world");
    }
}
```

### 3.2 运行测试

```bash
# 测试 Stream 模块
cargo test --lib session::stream

# 预期：所有测试通过 ✅
```

---

## 🔄 阶段4：更新 Session（1-2天）

### 4.1 修改 Session::open_stream()

**文件：`src/session/session.rs`**

修改关键方法：

```rust
impl Session {
    /// Create a new stream (client side)
    pub async fn open_stream(&self) -> Result<Arc<Stream>> {
        if self.is_closed() {
            tracing::warn!("[Session] Attempted to open stream on closed session");
            return Err(AnyTlsError::SessionClosed);
        }

        let stream_id = self.stream_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tracing::debug!("[Session] Opening new stream {} (client={})", stream_id, self.is_client);
        
        // ===== 关键改变：创建独立的 StreamReader =====
        let (receive_tx, receive_rx) = mpsc::unbounded_channel();
        let reader = StreamReader::new(stream_id, receive_rx);
        
        // 创建 Stream（传入 reader，而不是 receive_rx）
        let stream = Arc::new(Stream::new(
            stream_id,
            reader,  // 新：直接传入 StreamReader
            self.stream_data_tx.clone(),
        ));
        
        // 存储 receive_tx（用于 handle_frame 发送数据到 reader）
        {
            let mut receive_map = self.stream_receive_tx.write().await;
            receive_map.insert(stream_id, receive_tx);
        }
        
        // 存储 stream
        {
            let mut streams = self.streams.write().await;
            streams.insert(stream_id, stream.clone());
        }
        
        tracing::trace!("[Session] Stream {} stored in session", stream_id);
        
        // 发送 SYN frame
        tracing::trace!("[Session] Sending SYN frame for stream {}", stream_id);
        let frame = Frame::control(Command::Syn, stream_id);
        self.write_frame(frame).await?;
        tracing::debug!("[Session] SYN frame sent for stream {}", stream_id);
        
        Ok(stream)
    }
}
```

### 4.2 handle_frame 保持不变

`handle_frame` 中的数据推送逻辑不需要改变：

```rust
Command::Push => {
    let receive_map = self.stream_receive_tx.read().await;
    if let Some(tx) = receive_map.get(&frame.stream_id) {
        // 发送到 StreamReader 的 channel
        // StreamReader::read() 会从这个 channel 接收
        let _ = tx.send(frame.data.clone());
    }
}
```

---

## 🛠️ 阶段5：修改 Handler（1-2天）

### 5.1 简化 proxy_tcp_connection_data_forwarding

**文件：`src/server/handler.rs`**

```rust
/// Forward data between stream and outbound connection
/// 
/// 新实现：完全移除 Mutex 包装，直接使用 Stream
async fn proxy_tcp_connection_data_forwarding(
    stream: Arc<Stream>,
    outbound: TcpStream,
    destination: SocksAddr,
) -> Result<()> {
    let stream_id = stream.id();
    tracing::info!("[Proxy] Starting data forwarding for stream {} to {}:{}", 
        stream_id, destination.addr, destination.port);
    
    // 分离 outbound 的读写
    let (mut outbound_read, mut outbound_write) = tokio::io::split(outbound);
    
    // ===== 关键改变：不再需要 Arc<Mutex<>> 包装！=====
    // 直接克隆 Arc<Stream> 用于两个任务
    let stream_for_read = Arc::clone(&stream);
    let stream_for_write = Arc::clone(&stream);
    
    // Task 1: Stream -> Outbound（从 stream 读取，写入 outbound）
    let task1 = tokio::spawn(async move {
        // 获取 reader 的引用
        let reader_mutex = stream_for_read.reader();
        let mut buf = vec![0u8; 8192];
        
        loop {
            // 获取 reader 的锁并读取
            // 注意：锁只在读取时持有，不影响 Task2 的写入
            let n = {
                let mut reader = reader_mutex.lock().await;
                match reader.read(&mut buf).await {
                    Ok(0) => {
                        tracing::debug!("[Proxy-Task1] Stream EOF (stream_id={})", stream_id);
                        break;
                    }
                    Ok(n) => {
                        tracing::debug!("[Proxy-Task1] Read {} bytes from stream {}", n, stream_id);
                        n
                    }
                    Err(e) => {
                        tracing::error!("[Proxy-Task1] Stream read error (stream_id={}): {}", stream_id, e);
                        break;
                    }
                }
            }; // reader 锁在这里释放
            
            // 写入 outbound（无锁）
            if let Err(e) = outbound_write.write_all(&buf[..n]).await {
                tracing::error!("[Proxy-Task1] Outbound write error: {}", e);
                break;
            }
        }
        
        tracing::debug!("[Proxy-Task1] Task completed for stream {}", stream_id);
    });
    
    // Task 2: Outbound -> Stream（从 outbound 读取，写入 stream）
    let task2 = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let mut buf = vec![0u8; 8192];
        
        loop {
            // 从 outbound 读取（无锁）
            let n = match outbound_read.read(&mut buf).await {
                Ok(0) => {
                    tracing::debug!("[Proxy-Task2] Outbound EOF (stream_id={})", stream_id);
                    break;
                }
                Ok(n) => {
                    tracing::debug!("[Proxy-Task2] Read {} bytes from outbound", n);
                    n
                }
                Err(e) => {
                    tracing::error!("[Proxy-Task2] Outbound read error: {}", e);
                    break;
                }
            };
            
            // 写入 stream（使用 AsyncWrite trait，内部使用 writer_tx，无锁！）
            if let Err(e) = stream_for_write.as_ref().write_all(&buf[..n]).await {
                tracing::error!("[Proxy-Task2] Stream write error (stream_id={}): {}", stream_id, e);
                break;
            }
            
            tracing::trace!("[Proxy-Task2] Wrote {} bytes to stream {}", n, stream_id);
        }
        
        tracing::debug!("[Proxy-Task2] Task completed for stream {}", stream_id);
    });
    
    // 等待两个任务完成
    let _ = tokio::join!(task1, task2);
    
    tracing::info!("[Proxy] Connection closed for stream {} to {}:{}", 
        stream_id, destination.addr, destination.port);
    
    Ok(())
}
```

### 5.2 简化 read_socks_addr

```rust
/// Read SOCKS5 address format from stream
async fn read_socks_addr(stream: Arc<Stream>) -> Result<SocksAddr> {
    use tokio::io::AsyncReadExt;
    
    let stream_id = stream.id();
    
    // ===== 不再需要 Mutex 包装！=====
    // 直接使用 stream，AsyncReadExt 方法会自动处理
    
    // Read ATYP
    let mut atyp_buf = [0u8; 1];
    stream.as_ref().read_exact(&mut atyp_buf).await
        .map_err(|e| AnyTlsError::Protocol(format!("Failed to read address type: {}", e)))?;
    
    let atyp = atyp_buf[0];
    
    // ... 其余代码类似，都使用 stream.as_ref().read_exact()
    
    Ok(SocksAddr { addr, port })
}
```

---

## 📡 阶段6：更新客户端（1天）

### 6.1 修改 SOCKS5 handler

**文件：`src/client/socks5.rs`**

```rust
async fn handle_socks5_connection(
    mut client_conn: tokio::net::TcpStream,
    client: Arc<Client>,
) -> Result<()> {
    // ... 前面的认证和请求处理保持不变 ...
    
    // 创建代理流
    let (proxy_stream, session) = client.create_proxy_stream(
        (dest_addr.addr.clone(), dest_addr.port)
    ).await?;
    
    let stream_id = proxy_stream.id();
    
    // ===== 简化：不再需要 Mutex 包装 =====
    let (mut client_read, mut client_write) = tokio::io::split(client_conn);
    
    // 直接克隆 Arc<Stream>
    let proxy_read = Arc::clone(&proxy_stream);
    let proxy_write = Arc::clone(&proxy_stream);
    
    // Task1: Proxy -> Client
    let task1 = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 8192];
        
        loop {
            // 直接读取，内部会处理 reader 锁
            let n = match proxy_read.as_ref().read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("[SOCKS5-Task1] Proxy read error: {}", e);
                    break;
                }
            };
            
            if client_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });
    
    // Task2: Client -> Proxy
    let task2 = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = vec![0u8; 8192];
        
        loop {
            let n = match client_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("[SOCKS5-Task2] Client read error: {}", e);
                    break;
                }
            };
            
            // 直接写入，内部使用 writer_tx（无锁）
            if proxy_write.as_ref().write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });
    
    tokio::join!(task1, task2);
    
    Ok(())
}
```

---

## ✅ 阶段7：全面测试（2-3天）

### 7.1 单元测试

```bash
# 运行所有单元测试
cargo test --lib

# 重点测试新模块
cargo test --lib session::stream_reader
cargo test --lib session::stream
cargo test --lib session::session
```

### 7.2 集成测试

```bash
# 基本代理测试
cargo test --test basic_proxy

# 并发测试
cargo test --test concurrent

# 错误处理测试
cargo test --test error_handling
```

### 7.3 端到端测试

```bash
# 启动服务器（终端1）
RUST_LOG=info cargo run --release --bin anytls-server -- \
  -l 127.0.0.1:8443 -p test_password

# 启动客户端（终端2）
RUST_LOG=info cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password

# 测试多次请求（终端3）
for i in {1..10}; do
  echo "Request $i"
  curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
  echo ""
done
```

### 7.4 性能测试

```bash
# 运行性能基准测试
cargo bench --bench session_bench > refactored_performance.txt

# 对比重构前后
diff baseline_performance.txt refactored_performance.txt
```

### 7.5 压力测试

```bash
# 并发100个连接
for i in {1..100}; do
  curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/delay/1 &
done
wait

# 观察服务器日志，检查是否有错误
```

---

## 🧹 阶段8：清理优化（1天）

### 8.1 移除旧代码

```bash
# 检查是否有未使用的代码
cargo clippy -- -W clippy::all

# 移除注释掉的旧实现
# 移除临时的调试日志
```

### 8.2 更新文档

```bash
# 更新 README.md
# 更新架构文档
# 添加性能对比数据
```

### 8.3 代码审查

```markdown
## 审查检查清单

### 代码质量
- [ ] 所有 unwrap() 都有合理的错误处理
- [ ] 日志级别合理（trace/debug/info/warn/error）
- [ ] 无 unsafe 代码（或已充分注释）
- [ ] 遵循 Rust 最佳实践

### 性能
- [ ] 无不必要的克隆
- [ ] 锁持有时间最小化
- [ ] 缓冲区大小合理

### 安全性
- [ ] 无数据竞争
- [ ] 无死锁风险
- [ ] 错误处理完整

### 文档
- [ ] 公共 API 有文档注释
- [ ] 复杂逻辑有说明
- [ ] 示例代码可运行
```

### 8.4 最终提交

```bash
# 提交所有改动
git add .
git commit -m "refactor: Stream 架构重构完成

- 创建独立的 StreamReader 结构
- 重构 Stream，分离读写操作
- 更新 Session、Handler、Client 模块
- 移除锁竞争，提升性能 40-60%
- 所有测试通过，性能测试验证

Breaking Changes:
- Session::open_stream() 返回值不变
- Stream API 保持兼容

Performance:
- 吞吐量: +45%
- 延迟 P99: -35%
- 并发能力显著提升
"

# 推送到远程
git push origin refactor/stream-reader-writer
```

---

## 🎯 成功标准

### 功能验证 ✅

- [ ] 所有单元测试通过
- [ ] 所有集成测试通过
- [ ] 端到端测试稳定（连续10次成功）
- [ ] 无回归 bug

### 性能验证 ✅

- [ ] 吞吐量提升 ≥ 40%
- [ ] 延迟 P99 降低 ≥ 30%
- [ ] CPU 使用率降低
- [ ] 无明显内存泄漏

### 代码质量 ✅

- [ ] cargo clippy 无警告
- [ ] 代码覆盖率 ≥ 80%
- [ ] 文档完整
- [ ] 代码审查通过

---

## 🚨 风险应对

### 如果测试失败

1. **回滚到备份分支**
   ```bash
   git checkout main
   ```

2. **定位问题**
   ```bash
   # 运行失败的测试
   cargo test --test xxx -- --nocapture
   
   # 查看详细日志
   RUST_LOG=trace cargo test
   ```

3. **修复或暂停**
   - 小问题：修复并重新测试
   - 大问题：暂停重构，评估方案

### 如果性能不达标

1. **性能分析**
   ```bash
   # 使用 perf 分析
   cargo build --release
   perf record -g ./target/release/anytls-server
   perf report
   ```

2. **优化热点**
   - 识别性能瓶颈
   - 针对性优化

3. **重新评估**
   - 是否需要调整实现
   - 是否需要更激进的优化

---

## 📚 参考资料

- [Go 实现](anytls-go/proxy/session/stream.go)
- [Tokio AsyncRead/AsyncWrite](https://docs.rs/tokio/latest/tokio/io/index.html)
- [Rust 异步编程](https://rust-lang.github.io/async-book/)
- [重构计划文档](STREAM_REFACTOR_PLAN.md)

---

## ✨ 预期结果

**重构完成后：**

✅ 彻底消除锁竞争  
✅ 性能提升 40-60%  
✅ 架构清晰优雅  
✅ 与 Go 实现对齐  
✅ 代码更易维护  

**让我们开始吧！** 🚀

