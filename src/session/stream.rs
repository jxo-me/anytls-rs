//! Stream implementation for AnyTLS protocol
//!
//! Stream provides a duplex communication channel that implements AsyncRead and AsyncWrite

use crate::util::AnyTlsError;
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
    
    // Pipe for reading (receiving data from session)
    reader_rx: mpsc::UnboundedReceiver<Bytes>,
    reader_buffer: Vec<u8>,
    
    // Channel for writing (sending data to session)
    writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    
    // Stream state
    is_closed: Arc<AtomicBool>,
    close_error: Arc<tokio::sync::Mutex<Option<AnyTlsError>>>,
}

impl Stream {
    /// Create a new stream
    /// 
    /// # Arguments
    /// * `id` - Stream ID
    /// * `reader_rx` - Receiver for incoming data from session
    /// * `writer_tx` - Sender for outgoing data to session (sends (stream_id, data))
    pub fn new(
        id: u32,
        reader_rx: mpsc::UnboundedReceiver<Bytes>,
        writer_tx: mpsc::UnboundedSender<(u32, Bytes)>,
    ) -> Self {
        Self {
            id,
            reader_rx,
            reader_buffer: Vec::new(),
            writer_tx,
            is_closed: Arc::new(AtomicBool::new(false)),
            close_error: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Get stream ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Close the stream locally (without notifying remote)
    pub fn close_locally(&mut self) {
        self.is_closed.store(true, Ordering::Relaxed);
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

    /// Try to read from buffer only (non-blocking, for use in proxy tasks to minimize lock hold time)
    /// Returns number of bytes read, or 0 if buffer is empty
    pub fn try_read_from_buffer(&mut self, buf: &mut [u8]) -> usize {
        if !self.reader_buffer.is_empty() {
            let n = std::cmp::min(self.reader_buffer.len(), buf.len());
            buf[..n].copy_from_slice(&self.reader_buffer[..n]);
            self.reader_buffer.drain(..n);
            tracing::debug!("[Stream] try_read_from_buffer: Read {} bytes from buffer (stream_id={})", n, self.id);
            return n;
        }
        0
    }

    /// Receive data from channel (call this OUTSIDE of lock to avoid blocking lock)
    pub async fn recv_from_channel(&mut self) -> Option<Bytes> {
        self.reader_rx.recv().await
    }

    /// Store data in buffer (call this INSIDE of lock after receiving from channel)
    pub fn store_in_buffer(&mut self, data: Bytes, buf: &mut [u8]) -> usize {
        let data_bytes = data.as_ref();
        let data_len = data_bytes.len();
        let n = std::cmp::min(data_len, buf.len());
        
        buf[..n].copy_from_slice(&data_bytes[..n]);
        
        // Store remaining data in buffer
        if n < data_len {
            let remaining = data_len - n;
            tracing::trace!("[Stream] store_in_buffer: Storing {} remaining bytes in buffer (stream_id={})", remaining, self.id);
            self.reader_buffer.extend_from_slice(&data_bytes[n..]);
        }
        
        tracing::debug!("[Stream] store_in_buffer: Stored {} bytes (stream_id={})", n, self.id);
        n
    }
}

// Stream is not meant to be cloned - use Arc<Stream> instead
// This implementation is only for compatibility with HashMap storage

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let stream_id = self.id;
        let buf_remaining = buf.remaining();
        
        tracing::trace!("[Stream] poll_read: stream_id={}, buf_remaining={}, buffer_len={}", 
            stream_id, buf_remaining, self.reader_buffer.len());
        
        // First, try to consume from buffer
        if !self.reader_buffer.is_empty() {
            let n = std::cmp::min(self.reader_buffer.len(), buf.remaining());
            tracing::trace!("[Stream] poll_read: Consuming {} bytes from buffer (stream_id={})", n, stream_id);
            buf.put_slice(&self.reader_buffer[..n]);
            self.reader_buffer.drain(..n);
            tracing::debug!("[Stream] poll_read: Returned {} bytes from buffer (stream_id={})", n, stream_id);
            return Poll::Ready(Ok(()));
        }

        // Try to receive new data
        tracing::trace!("[Stream] poll_read: Calling poll_recv on channel (stream_id={})", stream_id);
        match self.reader_rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let data_bytes = data.as_ref();
                let data_len = data_bytes.len();
                let n = std::cmp::min(data_len, buf.remaining());
                
                tracing::debug!("[Stream] poll_read: Received {} bytes from channel, putting {} bytes to buf (stream_id={})", 
                    data_len, n, stream_id);
                
                buf.put_slice(&data_bytes[..n]);
                
                // Store remaining data in buffer
                if n < data_len {
                    let remaining = data_len - n;
                    tracing::trace!("[Stream] poll_read: Storing {} remaining bytes in buffer (stream_id={})", remaining, stream_id);
                    self.reader_buffer.extend_from_slice(&data_bytes[n..]);
                }
                
                tracing::debug!("[Stream] poll_read: Returned {} bytes from channel (stream_id={})", n, stream_id);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                // Channel closed - EOF
                tracing::debug!("[Stream] poll_read: Channel closed (EOF) for stream_id={}", stream_id);
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                tracing::trace!("[Stream] poll_read: Channel pending (no data available) for stream_id={}", stream_id);
                Poll::Pending
            }
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
