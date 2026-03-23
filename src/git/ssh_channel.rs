use std::io;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll};
use russh::{ChannelId, server::Handle};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex};

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
        eprintln!("[SSH-WRITER] poll_write called with {} bytes", buf.len());
        
        if let Some(ref mut fut) = self.pending {
            eprintln!("[SSH-WRITER] polling pending future");
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    eprintln!("[SSH-WRITER] pending future completed");
                    self.pending = None;
                    let len = self.pending_len;
                    self.pending_len = 0;
                    Poll::Ready(Ok(len))
                }
                Poll::Pending => {
                    eprintln!("[SSH-WRITER] pending future still pending");
                    Poll::Pending
                }
            }
        } else {
            let channel = self.channel;
            let session_handle = self.session_handle.clone();
            let data = bytes::Bytes::copy_from_slice(buf);
            let len = buf.len();
            let waker = cx.waker().clone();

            eprintln!("[SSH-WRITER] creating new future for {} bytes", len);
            eprintln!("[SSH-WRITER] data hex: {:02x?}", &buf[..len.min(120)]);
            eprintln!("[SSH-WRITER] data content: {:?}", String::from_utf8_lossy(&buf[..len.min(100)]));
            let mut fut = Box::pin(async move {
                eprintln!("[SSH-WRITER] inside future, sending data...");
                match session_handle.data(channel, data).await {
                    Ok(_) => eprintln!("[SSH-WRITER] data sent successfully"),
                    Err(e) => {
                        eprintln!("[SSH-WRITER] FAILED to send SSH data");
                        eprintln!("[SSH-WRITER] Error type: {}", std::any::type_name_of_val(&e));
                        eprintln!("[SSH-WRITER] Error debug: {:?}", e);
                    }
                }
                eprintln!("[SSH-WRITER] data sent, waking");
                waker.wake();
            });
            
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    eprintln!("[SSH-WRITER] future completed immediately");
                    Poll::Ready(Ok(len))
                }
                Poll::Pending => {
                    eprintln!("[SSH-WRITER] future pending, storing");
                    self.pending = Some(fut);
                    self.pending_len = len;
                    Poll::Pending
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        eprintln!("[SSH-WRITER] poll_flush called, pending={:?}", self.pending.is_some());
        if let Some(ref mut fut) = self.pending {
            match fut.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    eprintln!("[SSH-WRITER] flush: pending future completed");
                    self.pending = None;
                    self.pending_len = 0;
                    Poll::Ready(Ok(()))
                }
                Poll::Pending => {
                    eprintln!("[SSH-WRITER] flush: pending future still pending");
                    Poll::Pending
                }
            }
        } else {
            eprintln!("[SSH-WRITER] flush: no pending future");
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        eprintln!("[SSH-WRITER] poll_shutdown called");
        self.as_mut().poll_flush(cx)
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
        eprintln!("[SSH-READER] poll_read called, buffer remaining={}", buf.remaining());
        
        if self.offset < self.buffer.len() {
            let remaining = &self.buffer[self.offset..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.offset += to_copy;

            if self.offset >= self.buffer.len() {
                self.buffer.clear();
                self.offset = 0;
            }

            eprintln!("[SSH-READER] returned {} bytes from buffer", to_copy);
            return Poll::Ready(Ok(()));
        }

        let receiver = self.receiver.clone();
        let fut = receiver.lock();
        tokio::pin!(fut);

        let mut receiver_guard: tokio::sync::MutexGuard<'_, mpsc::UnboundedReceiver<Vec<u8>>> =
            match fut.as_mut().poll(cx) {
                Poll::Ready(guard) => guard,
                Poll::Pending => {
                    eprintln!("[SSH-READER] mutex lock pending");
                    return Poll::Pending;
                }
            };

        match receiver_guard.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                eprintln!("[SSH-READER] received {} bytes from channel", data.len());
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
                eprintln!("[SSH-READER] channel closed");
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                eprintln!("[SSH-READER] no data available yet");
                Poll::Pending
            }
        }
    }
}
