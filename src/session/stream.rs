//! Stream implementation for AnyTLS protocol
//!
//! Stream provides a duplex communication channel that implements AsyncRead and AsyncWrite

use crate::session::StreamReader;
use crate::util::{AnyTlsError, Result};
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, oneshot};

type PendingWrite = Pin<
    Box<
        dyn Future<
                Output = std::result::Result<
                    mpsc::OwnedPermit<(u32, Bytes)>,
                    mpsc::error::SendError<()>,
                >,
            > + Send,
    >,
>;

/// Stream represents a single data stream within a Session
/// It implements AsyncRead and AsyncWrite to be used as a connection
pub struct Stream {
    id: u32,

    // ===== 读取部分：使用独立的 StreamReader =====
    // Arc<Mutex<>> 是为了在 poll_read 中获取 &mut
    reader: Arc<tokio::sync::Mutex<StreamReader>>,

    // ===== 写入部分：直接使用 channel，无需锁 =====
    writer_tx: mpsc::Sender<(u32, Bytes)>,
    pending_write: std::sync::Mutex<Option<PendingWrite>>,

    // ===== SYNACK 通知 (用于超时检测) =====
    synack_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<()>>>>>,

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
    ///
    /// # Returns
    /// (Stream, Receiver) - The receiver can be used to wait for SYNACK
    pub fn new(
        id: u32,
        reader: StreamReader,
        writer_tx: mpsc::Sender<(u32, Bytes)>,
    ) -> (Self, oneshot::Receiver<Result<()>>) {
        let (synack_tx, synack_rx) = oneshot::channel();

        let stream = Self {
            id,
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            writer_tx,
            pending_write: std::sync::Mutex::new(None),
            synack_tx: Arc::new(tokio::sync::Mutex::new(Some(synack_tx))),
            is_closed: Arc::new(AtomicBool::new(false)),
            close_error: Arc::new(tokio::sync::Mutex::new(None)),
        };

        (stream, synack_rx)
    }

    /// Notify that SYNACK has been received
    ///
    /// # Arguments
    /// * `result` - Ok(()) for success, Err for error
    pub async fn notify_synack(&self, result: Result<()>) {
        let mut tx_guard = self.synack_tx.lock().await;
        match tx_guard.take() {
            Some(tx) => {
                let result_clone = match &result {
                    Ok(()) => Ok(()),
                    Err(e) => Err(AnyTlsError::Protocol(e.to_string())),
                };
                let _ = tx.send(result_clone);
                tracing::debug!(
                    "[Stream] SYNACK notified for stream {}: {:?}",
                    self.id,
                    result.is_ok()
                );
            }
            _ => {
                tracing::warn!("[Stream] SYNACK already notified for stream {}", self.id);
            }
        }
    }

    /// Get stream ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Close the stream with error (can be called with `Arc<Stream>`)
    pub async fn close_with_error(&self, err: AnyTlsError) {
        if self
            .is_closed
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            tracing::warn!(
                stream_id = self.id,
                cause = %err,
                "[Stream] Closing stream with error"
            );
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

    /// Send data through the bounded writer channel, waiting for available capacity.
    pub async fn send_data(
        &self,
        data: Bytes,
    ) -> std::result::Result<(), mpsc::error::SendError<(u32, Bytes)>> {
        self.writer_tx.send((self.id, data)).await
    }

    /// Try to send without waiting for queue capacity.
    ///
    /// Returns the data when the bounded queue is full, allowing callers to retry or drop it.
    pub fn try_send_data(
        &self,
        data: Bytes,
    ) -> std::result::Result<(), mpsc::error::TrySendError<(u32, Bytes)>> {
        self.writer_tx.try_send((self.id, data))
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
        let remaining = buf.remaining();

        // 使用 tokio::task::block_in_place 同步读取
        // 这避免了复杂的 Future polling 和借用问题
        let reader = Arc::clone(&self.reader);

        // 创建读取 future
        let mut read_fut = Box::pin(async move {
            let mut reader_guard = reader.lock().await;
            let mut temp_buf = vec![0u8; remaining];
            let n = reader_guard.read(&mut temp_buf).await?;
            Ok::<(usize, Vec<u8>), std::io::Error>((n, temp_buf))
        });

        // Poll the future
        match read_fut.as_mut().poll(cx) {
            Poll::Ready(Ok((n, temp_buf))) => {
                if n > 0 {
                    buf.put_slice(&temp_buf[..n]);
                    tracing::trace!(
                        "[Stream] poll_read: Read {} bytes (stream_id={})",
                        n,
                        stream_id
                    );
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => {
                tracing::error!(
                    "[Stream] poll_read: Error reading (stream_id={}): {}",
                    stream_id,
                    e
                );
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let stream_id = self.id;
        let buf_len = buf.len();
        tracing::trace!(
            "[Stream] poll_write: stream_id={}, buf_len={}",
            stream_id,
            buf_len
        );

        if self.is_closed.load(Ordering::Relaxed) {
            tracing::warn!("[Stream] poll_write: Stream {} is closed", stream_id);
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream closed",
            )));
        }

        // Send data to session via channel
        let data = Bytes::copy_from_slice(buf);
        tracing::trace!(
            "[Stream] poll_write: Sending {} bytes to channel for stream {}",
            buf_len,
            stream_id
        );
        let this = self.as_mut().get_mut();
        let mut pending_write = this.pending_write.lock().unwrap();
        if pending_write.is_none() {
            *pending_write = Some(Box::pin(this.writer_tx.clone().reserve_owned()));
        }
        match pending_write.as_mut().unwrap().as_mut().poll(_cx) {
            Poll::Ready(Ok(permit)) => {
                *pending_write = None;
                permit.send((this.id, data));
                tracing::debug!(
                    "[Stream] poll_write: Successfully sent {} bytes to channel for stream {}",
                    buf_len,
                    stream_id
                );
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(_)) => {
                *pending_write = None;
                tracing::error!(
                    "[Stream] poll_write: Failed to send {} bytes to channel for stream {}: {:?}",
                    buf_len,
                    stream_id,
                    "writer channel closed"
                );
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "session channel closed",
                )))
            }
            Poll::Pending => Poll::Pending,
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
        let (tx, mut rx) = mpsc::channel(2);
        let (_reader_tx, reader_rx) = mpsc::channel(2);

        let reader = StreamReader::new(1, reader_rx);
        let (mut stream, _synack_rx) = Stream::new(1, reader, tx);

        // 写入数据
        stream.write_all(b"hello").await.unwrap();

        // 验证数据发送到 channel
        let (stream_id, data) = rx.recv().await.unwrap();
        assert_eq!(stream_id, 1);
        assert_eq!(data.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn test_stream_write_applies_backpressure_when_queue_is_full() {
        let (tx, mut rx) = mpsc::channel(1);
        let (_reader_tx, reader_rx) = mpsc::channel(1);
        let reader = StreamReader::new(1, reader_rx);
        let (mut stream, _synack_rx) = Stream::new(1, reader, tx);

        stream.write_all(b"first").await.unwrap();
        let blocked = tokio::spawn(async move {
            stream.write_all(b"second").await.unwrap();
        });
        tokio::task::yield_now().await;
        assert!(
            !blocked.is_finished(),
            "write should wait for queue capacity"
        );

        let (id, first) = rx.recv().await.unwrap();
        assert_eq!(id, 1);
        assert_eq!(first.as_ref(), b"first");
        blocked.await.unwrap();
        let (id, second) = rx.recv().await.unwrap();
        assert_eq!(id, 1);
        assert_eq!(second.as_ref(), b"second");
    }

    #[tokio::test]
    async fn test_stream_read() {
        let (tx, _rx) = mpsc::channel(2);
        let (reader_tx, reader_rx) = mpsc::channel(2);

        let reader = StreamReader::new(1, reader_rx);
        let (mut stream, _synack_rx) = Stream::new(1, reader, tx);

        // 发送数据到 reader
        reader_tx.send(Bytes::from("world")).await.unwrap();

        // 读取数据
        let mut buf = vec![0u8; 10];
        let n = stream.read(&mut buf).await.unwrap();

        assert_eq!(n, 5);
        assert_eq!(&buf[..n], b"world");
    }

    #[tokio::test]
    async fn test_stream_read_write() {
        let (tx, mut rx) = mpsc::channel(2);
        let (reader_tx, reader_rx) = mpsc::channel(2);

        let reader = StreamReader::new(1, reader_rx);
        let (mut stream, _synack_rx) = Stream::new(1, reader, tx);

        // 同时读写
        reader_tx.send(Bytes::from("input")).await.unwrap();
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
