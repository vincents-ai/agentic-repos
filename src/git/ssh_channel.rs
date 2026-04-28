use russh::{server::Handle, ChannelId};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

pub struct SshChannelWriter {
    channel: ChannelId,
    session_handle: Handle,
    pending: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    pending_len: usize,
}

impl SshChannelWriter {
    pub fn new(channel: ChannelId, session_handle: Handle) -> Self {
        Self {
            channel,
            session_handle,
            pending: None,
            pending_len: 0,
        }
    }
}

impl AsyncWrite for SshChannelWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(ref mut fut) = self.pending {
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    self.pending = None;
                    let len = self.pending_len;
                    self.pending_len = 0;
                    Poll::Ready(Ok(len))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            let channel = self.channel;
            let session_handle = self.session_handle.clone();
            let data = bytes::Bytes::copy_from_slice(buf);
            let len = buf.len();

            let mut fut = Box::pin(async move {
                let _ = session_handle.data(channel, data).await;
            });

            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => Poll::Ready(Ok(len)),
                Poll::Pending => {
                    self.pending = Some(fut);
                    self.pending_len = len;
                    Poll::Pending
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(ref mut fut) = self.pending {
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    self.pending = None;
                    self.pending_len = 0;
                    Poll::Ready(Ok(()))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Flush any pending writes, then signal EOF on the channel.
        if let Some(ref mut fut) = self.pending {
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    self.pending = None;
                    self.pending_len = 0;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        // Send EOF via the SSH channel. We can't await here (poll_shutdown is sync),
        // so spawn a task to send the EOF signal.
        let handle = self.session_handle.clone();
        let channel = self.channel;
        // Use a oneshot to track completion
        let waker = cx.waker().clone();
        tokio::spawn(async move {
            let _ = handle.eof(channel).await;
            waker.wake();
        });
        Poll::Pending // Will be woken when the spawned task completes
    }
}

pub struct SshChannelReader {
    receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    buffer: Vec<u8>,
    offset: usize,
}

impl SshChannelReader {
    pub fn new(receiver: mpsc::UnboundedReceiver<Vec<u8>>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(receiver)),
            buffer: Vec::new(),
            offset: 0,
        }
    }
}

impl AsyncRead for SshChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.offset < self.buffer.len() {
            let remaining = &self.buffer[self.offset..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.offset += to_copy;

            if self.offset >= self.buffer.len() {
                self.buffer.clear();
                self.offset = 0;
            }

            return Poll::Ready(Ok(()));
        }

        let receiver = self.receiver.clone();
        let fut = receiver.lock();
        tokio::pin!(fut);

        let mut receiver_guard: tokio::sync::MutexGuard<'_, mpsc::UnboundedReceiver<Vec<u8>>> =
            match fut.as_mut().poll(cx) {
                Poll::Ready(guard) => guard,
                Poll::Pending => {
                    return Poll::Pending;
                }
            };

        match receiver_guard.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                if data.is_empty() {
                    return Poll::Ready(Ok(()));
                }

                let to_copy = data.len().min(buf.remaining());
                buf.put_slice(&data[..to_copy]);

                if to_copy < data.len() {
                    self.buffer = data;
                    self.offset = to_copy;
                }

                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                debug!("SSH channel reader: sender dropped, returning EOF");
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
