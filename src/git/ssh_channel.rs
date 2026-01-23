use russh::server::Handle;
use russh::{ChannelId, CryptoVec};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

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

        let data_result = {
            let mut receiver_guard = match self.receiver.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            };
            receiver_guard.poll_recv(cx)
        }; // Guard dropped here

        match data_result {
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
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct SshChannelWriter {
    channel: ChannelId,
    session_handle: Handle,
    pending_write: Option<tokio::task::JoinHandle<()>>,
}

impl SshChannelWriter {
    pub fn new(channel: ChannelId, session_handle: Handle) -> Self {
        Self {
            channel,
            session_handle,
            pending_write: None,
        }
    }
}

impl AsyncWrite for SshChannelWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(task) = &mut self.pending_write {
            match Pin::new(task).poll(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending_write = None;
                }
                Poll::Ready(Err(_)) => {
                    self.pending_write = None;
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        let channel = self.channel;
        let session_handle = self.session_handle.clone();
        let data: CryptoVec = buf.to_vec().into();
        let len = buf.len();

        let waker = cx.waker().clone();
        let task = tokio::spawn(async move {
            if let Err(e) = session_handle.data(channel, data).await {
                eprintln!("Failed to send SSH data: {:?}", e);
            }
            waker.wake();
        });

        self.pending_write = Some(task);
        Poll::Ready(Ok(len))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(task) = &mut self.pending_write {
            match Pin::new(task).poll(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending_write = None;
                }
                Poll::Ready(Err(_)) => {
                    self.pending_write = None;
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}
