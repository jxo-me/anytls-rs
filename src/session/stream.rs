//! Stream implementation for AnyTLS protocol
//!
//! Stream provides a duplex communication channel that implements AsyncRead and AsyncWrite

use crate::util::AnyTlsError;
use crate::session::StreamReader;
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
    // Arc<Mutex<>> 是为了在 poll_read 中获取 &mut
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
    
    /// Get a reference to the reader (for direct access in handlers)
    pub fn reader(&self) -> &Arc<tokio::sync::Mutex<StreamReader>> {
        &self.reader
    }
}

// Stream is not meant to be cloned - use Arc<Stream> instead
// This implementation is only for compatibility with HashMap storage

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let stream_id = self.id;
        
        // 获取 reader 的锁
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
        tracing::trace!("[Stream] poll_write: stream_id={}, buf_len={}", stream_id, buf_len);
        
        if self.is_closed.load(Ordering::Relaxed) {
            tracing::warn!("[Stream] poll_write: Stream {} is closed", stream_id);
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream closed",
            )));
        }

        // Send data to session via channel
        let data = Bytes::copy_from_slice(buf);
        tracing::trace!("[Stream] poll_write: Sending {} bytes to channel for stream {}", buf_len, stream_id);
        match self.writer_tx.send((self.id, data)) {
            Ok(_) => {
                tracing::debug!("[Stream] poll_write: Successfully sent {} bytes to channel for stream {}", buf_len, stream_id);
                Poll::Ready(Ok(buf.len()))
            }
            Err(e) => {
                tracing::error!("[Stream] poll_write: ❌ Failed to send {} bytes to channel for stream {}: {:?}", buf_len, stream_id, e);
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
        // Mark as closed
        self.is_closed.store(true, Ordering::Relaxed);
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::StreamReader;
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
    
    #[tokio::test]
    async fn test_stream_read_write() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (reader_tx, reader_rx) = mpsc::unbounded_channel();
        
        let reader = StreamReader::new(1, reader_rx);
        let mut stream = Stream::new(1, reader, tx);
        
        // 同时读写
        reader_tx.send(Bytes::from("input")).unwrap();
        stream.write_all(b"output").await.unwrap();
        
        // 验证读取
        let mut buf = vec![0u8; 10];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"input");
        
        // 验证写入
        let (stream_id, data) = rx.recv().await.unwrap();
        assert_eq!(stream_id, 1);
        assert_eq!(data.as_ref(), b"output");
    }
}
