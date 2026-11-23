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
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, oneshot};

#[doc(hidden)]
pub enum StreamCommand {
    Data(u32, Bytes),
    Fin(u32),
}

type PendingPermit = Pin<
    Box<
        dyn Future<
                Output = std::result::Result<
                    mpsc::OwnedPermit<StreamCommand>,
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
    writer_tx: mpsc::Sender<StreamCommand>,
    pending_write: std::sync::Mutex<Option<PendingPermit>>,
    pending_fin: std::sync::Mutex<Option<PendingPermit>>,
    write_waker: std::sync::Mutex<Option<Waker>>,
    read_waker: std::sync::Mutex<Option<Waker>>,

    // ===== SYNACK 通知 (用于超时检测) =====
    synack_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<()>>>>>,

    // ===== 状态管理 =====
    is_closed: Arc<AtomicBool>,
    read_aborted: AtomicBool,
    fin_sent: AtomicBool,
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
        writer_tx: mpsc::Sender<StreamCommand>,
    ) -> (Self, oneshot::Receiver<Result<()>>) {
        let (synack_tx, synack_rx) = oneshot::channel();

        let stream = Self {
            id,
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
            writer_tx,
            pending_write: std::sync::Mutex::new(None),
            pending_fin: std::sync::Mutex::new(None),
            write_waker: std::sync::Mutex::new(None),
            read_waker: std::sync::Mutex::new(None),
            synack_tx: Arc::new(tokio::sync::Mutex::new(Some(synack_tx))),
            is_closed: Arc::new(AtomicBool::new(false)),
            read_aborted: AtomicBool::new(false),
            fin_sent: AtomicBool::new(false),
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
            self.read_aborted.store(true, Ordering::Release);
            // A pending reserve owns a sender clone. Cancel it so removing the
            // session's sender closes the reader channel after queued data drains.
            *self.pending_write.lock().unwrap() = None;
            *self.pending_fin.lock().unwrap() = None;
            if let Some(waker) = self.read_waker.lock().unwrap().take() {
                waker.wake();
            }
            if let Some(waker) = self.write_waker.lock().unwrap().take() {
                waker.wake();
            }
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
        if self.is_closed.load(Ordering::Acquire) || self.read_aborted.load(Ordering::Acquire) {
            return Err(mpsc::error::SendError((self.id, data)));
        }
        self.writer_tx
            .send(StreamCommand::Data(self.id, data))
            .await
            .map_err(|error| match error.0 {
                StreamCommand::Data(id, data) => mpsc::error::SendError((id, data)),
                StreamCommand::Fin(_) => unreachable!("send_data only sends data"),
            })
    }

    /// Try to send without waiting for queue capacity.
    ///
    /// Returns the data when the bounded queue is full, allowing callers to retry or drop it.
    pub fn try_send_data(
        &self,
        data: Bytes,
    ) -> std::result::Result<(), mpsc::error::TrySendError<(u32, Bytes)>> {
        if self.is_closed.load(Ordering::Acquire) || self.read_aborted.load(Ordering::Acquire) {
            return Err(mpsc::error::TrySendError::Closed((self.id, data)));
        }
        self.writer_tx
            .try_send(StreamCommand::Data(self.id, data))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(StreamCommand::Data(id, data)) => {
                    mpsc::error::TrySendError::Full((id, data))
                }
                mpsc::error::TrySendError::Closed(StreamCommand::Data(id, data)) => {
                    mpsc::error::TrySendError::Closed((id, data))
                }
                _ => unreachable!("try_send_data only sends data"),
            })
    }

    /// Send a FIN after all previously queued stream data.
    pub async fn send_fin(&self) -> std::result::Result<(), mpsc::error::SendError<StreamCommand>> {
        if self.fin_sent.load(Ordering::Acquire) {
            return Ok(());
        }
        if self.read_aborted.load(Ordering::Acquire) {
            return Err(mpsc::error::SendError(StreamCommand::Fin(self.id)));
        }
        self.writer_tx.send(StreamCommand::Fin(self.id)).await?;
        self.fin_sent.store(true, Ordering::Release);
        self.is_closed.store(true, Ordering::Release);
        Ok(())
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
        if self.read_aborted.load(Ordering::Acquire) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stream closed with error",
            )));
        }
        *self.read_waker.lock().unwrap() = Some(cx.waker().clone());
        let remaining = buf.remaining();

        // 使用 tokio::task::block_in_place 同步读取
        // 这避免了复杂的 Future polling 和借用问题
        let reader = Arc::clone(&self.reader);

        // 创建读取 future
        let mut read_fut = std::pin::pin!(async move {
            let mut reader_guard = reader.lock().await;
            let mut temp_buf = vec![0u8; remaining];
            let n = reader_guard.read(&mut temp_buf).await?;
            Ok::<(usize, Vec<u8>), std::io::Error>((n, temp_buf))
        });

        // Poll the future
        match read_fut.as_mut().poll(cx) {
            Poll::Ready(Ok((n, temp_buf))) => {
                self.read_waker.lock().unwrap().take();
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
                self.read_waker.lock().unwrap().take();
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
        *this.write_waker.lock().unwrap() = Some(_cx.waker().clone());
        let mut pending_write = this.pending_write.lock().unwrap();
        if pending_write.is_none() {
            *pending_write = Some(Box::pin(this.writer_tx.clone().reserve_owned()));
        }
        match pending_write.as_mut().unwrap().as_mut().poll(_cx) {
            Poll::Ready(Ok(permit)) => {
                *pending_write = None;
                this.write_waker.lock().unwrap().take();
                permit.send(StreamCommand::Data(this.id, data));
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

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.is_closed.load(Ordering::Acquire) {
            return Poll::Ready(Ok(()));
        }
        *this.write_waker.lock().unwrap() = Some(cx.waker().clone());
        let mut pending_fin = this.pending_fin.lock().unwrap();
        if pending_fin.is_none() {
            *pending_fin = Some(Box::pin(this.writer_tx.clone().reserve_owned()));
        }
        match pending_fin.as_mut().unwrap().as_mut().poll(cx) {
            Poll::Ready(Ok(permit)) => {
                *pending_fin = None;
                permit.send(StreamCommand::Fin(this.id));
                this.fin_sent.store(true, Ordering::Release);
                this.is_closed.store(true, Ordering::Release);
                this.write_waker.lock().unwrap().take();
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(_)) => {
                *pending_fin = None;
                this.is_closed.store(true, Ordering::Release);
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "session channel closed",
                )))
            }
            Poll::Pending => Poll::Pending,
        }
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
        let StreamCommand::Data(stream_id, data) = rx.recv().await.unwrap() else {
            panic!("expected stream data command");
        };
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

        let StreamCommand::Data(id, first) = rx.recv().await.unwrap() else {
            panic!("expected first stream data command");
        };
        assert_eq!(id, 1);
        assert_eq!(first.as_ref(), b"first");
        blocked.await.unwrap();
        let StreamCommand::Data(id, second) = rx.recv().await.unwrap() else {
            panic!("expected second stream data command");
        };
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
        let StreamCommand::Data(stream_id, data) = rx.recv().await.unwrap() else {
            panic!("expected stream data command");
        };
        assert_eq!(stream_id, 1);
        assert_eq!(data.as_ref(), b"output");
    }

    #[tokio::test]
    async fn shutdown_enqueues_fin_after_prior_stream_data() {
        let (tx, mut rx) = mpsc::channel(2);
        let (_reader_tx, reader_rx) = mpsc::channel(1);
        let reader = StreamReader::new(4, reader_rx);
        let (mut stream, _synack_rx) = Stream::new(4, reader, tx);

        stream.write_all(b"tail").await.unwrap();
        stream.shutdown().await.unwrap();

        assert!(matches!(
            rx.recv().await,
            Some(StreamCommand::Data(4, data)) if data == Bytes::from_static(b"tail")
        ));
        assert!(matches!(rx.recv().await, Some(StreamCommand::Fin(4))));
    }
}
