//! Local webhook receiver for E2E tests.
//!
//! Starts a lightweight HTTP server that captures webhook POST deliveries.
//! Tests can inspect received payloads to assert correct webhook firing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// A captured webhook delivery.
#[derive(Debug, Clone)]
pub struct CapturedDelivery {
    /// Request path (e.g., `/webhook`).
    pub path: String,
    /// HTTP method (e.g., POST).
    pub method: String,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Request body as bytes.
    pub body: Vec<u8>,
}

/// Local HTTP server that captures incoming webhook deliveries.
pub struct WebhookReceiver {
    /// Port the receiver is listening on.
    pub port: u16,
    /// Base URL for registering as a webhook target.
    pub url: String,
    /// All captured deliveries, in order received.
    deliveries: Arc<Mutex<Vec<CapturedDelivery>>>,
    /// Shutdown signal sender.
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl WebhookReceiver {
    /// Start a new webhook receiver on a random port.
    ///
    /// The receiver will capture all POST requests to `/webhook` and any sub-path.
    /// Returns the receiver handle for inspecting deliveries.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let url = format!("http://127.0.0.1:{}/webhook", port);

        let deliveries: Arc<Mutex<Vec<CapturedDelivery>>> = Arc::new(Mutex::new(Vec::new()));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let deliveries_clone = deliveries.clone();

        tokio::spawn(async move {
            tokio::select! {
                _ = Self::serve(listener, deliveries_clone) => {}
                _ = shutdown_rx => {}
            }
        });

        Ok(Self {
            port,
            url,
            deliveries,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    async fn serve(
        listener: TcpListener,
        deliveries: Arc<Mutex<Vec<CapturedDelivery>>>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };

            let deliveries = deliveries.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let n = match stream.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                buf.truncate(n);
                let request_str = String::from_utf8_lossy(&buf);

                // Parse HTTP request line
                let mut lines = request_str.lines();
                let request_line = lines.next().unwrap_or("");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("GET").to_string();
                let path = parts.next().unwrap_or("/").to_string();

                // Parse headers
                let mut headers = HashMap::new();
                for line in lines.by_ref() {
                    if line.is_empty() {
                        break; // End of headers
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        headers.insert(
                            key.trim().to_lowercase(),
                            value.trim().to_string(),
                        );
                    }
                }

                // Find body (after double CRLF)
                let body = if let Some(idx) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    buf[idx + 4..].to_vec()
                } else {
                    Vec::new()
                };

                let delivery = CapturedDelivery {
                    path,
                    method,
                    headers,
                    body,
                };

                deliveries.lock().unwrap().push(delivery);

                // Respond 200 OK
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    }

    /// Get all captured deliveries.
    pub fn deliveries(&self) -> Vec<CapturedDelivery> {
        self.deliveries.lock().unwrap().clone()
    }

    /// Get the number of captured deliveries.
    pub fn delivery_count(&self) -> usize {
        self.deliveries.lock().unwrap().len()
    }

    /// Wait for at least `n` deliveries, polling every 100ms up to `timeout`.
    pub async fn wait_for_deliveries(
        &self,
        n: usize,
        timeout: std::time::Duration,
    ) -> Result<Vec<CapturedDelivery>, String> {
        let start = std::time::Instant::now();
        loop {
            let count = self.delivery_count();
            if count >= n {
                return Ok(self.deliveries());
            }
            if start.elapsed() > timeout {
                return Err(format!(
                    "Timed out waiting for {} deliveries, got {}",
                    n, count
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Clear all captured deliveries.
    pub fn clear(&self) {
        self.deliveries.lock().unwrap().clear();
    }

    /// Parse the most recent delivery body as JSON.
    pub fn last_delivery_json(&self) -> Option<serde_json::Value> {
        let deliveries = self.deliveries.lock().unwrap();
        let last = deliveries.last()?;
        serde_json::from_slice(&last.body).ok()
    }
}

impl Drop for WebhookReceiver {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webhook_receiver_captures_post() {
        let receiver = WebhookReceiver::start().await.unwrap();

        // Send a POST to the receiver
        let client = reqwest::Client::new();
        let resp = client
            .post(&receiver.url)
            .json(&serde_json::json!({
                "event": "push",
                "data": {"ref": "refs/heads/main"}
            }))
            .send()
            .await
            .unwrap();

        assert!(resp.status().is_success());

        // Wait for delivery
        let deliveries = receiver
            .wait_for_deliveries(1, std::time::Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].method, "POST");
        assert_eq!(deliveries[0].path, "/webhook");

        let body: serde_json::Value = serde_json::from_slice(&deliveries[0].body).unwrap();
        assert_eq!(body["event"], "push");
    }

    #[tokio::test]
    async fn test_webhook_receiver_multiple_deliveries() {
        let receiver = WebhookReceiver::start().await.unwrap();
        let client = reqwest::Client::new();

        for i in 0..3 {
            client
                .post(&receiver.url)
                .json(&serde_json::json!({"index": i}))
                .send()
                .await
                .unwrap();
        }

        let deliveries = receiver
            .wait_for_deliveries(3, std::time::Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(deliveries.len(), 3);
    }
}
