//! SSE stream wrapper for actix-web streaming responses.

use crate::error::SseRegistryError;
use adp_proto::sse::SseEvent;
use futures_util::stream::{self, Stream};
use std::pin::Pin;
use tokio::sync::broadcast;
use tracing::warn;

/// A boxed SSE stream suitable for actix-web streaming responses.
pub type SseStream = Pin<Box<dyn Stream<Item = Result<web::Bytes, SseRegistryError>> + 'static>>;

use actix_web::web;

/// Create an SSE stream from a broadcast receiver.
pub fn create_stream(
    rx: broadcast::Receiver<SseEvent>,
) -> impl Stream<Item = Result<web::Bytes, SseRegistryError>> {
    stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = event.to_sse_data();
                    return Some((Ok(web::Bytes::from(data)), rx));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE receiver lagged, skipping events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return None;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stream_receives_events() {
        let (tx, rx) = broadcast::channel(16);
        let mut stream = std::pin::pin!(create_stream(rx));

        tx.send(SseEvent::Heartbeat {
            timestamp: "2026-04-23T14:30:00Z".to_string(),
        }).unwrap();

        use futures_util::StreamExt;
        let item = stream.next().await;
        assert!(item.is_some());
        let bytes = item.unwrap().unwrap();
        let s = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(s.contains("heartbeat"));
    }
}
