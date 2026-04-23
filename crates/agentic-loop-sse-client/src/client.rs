//! SSE client with auto-reconnection and exponential backoff.

use crate::config::SseClientConfig;
use crate::error::SseClientError;
use crate::handler::EventHandlers;
use crate::ConnectionState;
use adp_proto::sse::SseEvent;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

/// SSE client for connecting to an adp server.
pub struct SseClient {
    config: SseClientConfig,
    handlers: EventHandlers,
    state: Arc<RwLock<ConnectionState>>,
    running: Arc<AtomicBool>,
    http: reqwest::Client,
}

impl SseClient {
    /// Create a new SSE client.
    pub fn new(config: SseClientConfig) -> Self {
        Self {
            config,
            handlers: EventHandlers::new(),
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            running: Arc::new(AtomicBool::new(false)),
            http: reqwest::Client::new(),
        }
    }

    /// Add an event handler.
    pub fn add_handler(&mut self, handler: Box<dyn crate::handler::EventHandler>) {
        self.handlers.add(handler);
    }

    /// Get the current connection state.
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Check if the client is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Start the SSE client with auto-reconnection.
    ///
    /// This runs until stopped or max reconnection attempts exceeded.
    pub async fn run(&self) -> Result<(), SseClientError> {
        self.running.store(true, Ordering::Relaxed);
        let mut attempts = 0u32;
        let mut backoff_ms = self.config.initial_backoff_ms;

        while self.running.load(Ordering::Relaxed) {
            match self.connect_once().await {
                Ok(()) => {
                    // Connection closed normally, reset backoff
                    attempts = 0;
                    backoff_ms = self.config.initial_backoff_ms;
                }
                Err(SseClientError::TtlExpired { .. }) => {
                    // TTL expired, cannot reconnect without re-registration
                    warn!("TTL expired, stopping client");
                    *self.state.write().await = ConnectionState::Failed;
                    return Err(SseClientError::TtlExpired {
                        message: "secret expired, re-registration required".to_string(),
                    });
                }
                Err(SseClientError::AuthenticationRejected(reason)) => {
                    // Auth permanently failed
                    warn!(reason = %reason, "Authentication rejected, stopping client");
                    *self.state.write().await = ConnectionState::Failed;
                    return Err(SseClientError::AuthenticationRejected(reason));
                }
                Err(SseClientError::Stopped) => {
                    info!("Client stopped");
                    *self.state.write().await = ConnectionState::Disconnected;
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= self.config.max_reconnect_attempts {
                        error!(attempts = attempts, "Max reconnection attempts exceeded");
                        *self.state.write().await = ConnectionState::Failed;
                        return Err(SseClientError::MaxReconnectAttempts { attempts });
                    }

                    warn!(
                        error = %e,
                        attempt = attempts,
                        backoff_ms = backoff_ms,
                        "Connection failed, reconnecting"
                    );

                    *self.state.write().await = ConnectionState::Reconnecting;
                    time::sleep(Duration::from_millis(backoff_ms)).await;

                    // Exponential backoff
                    backoff_ms = ((backoff_ms as f64) * self.config.backoff_multiplier) as u64;
                    backoff_ms = backoff_ms.min(self.config.max_backoff_ms);
                }
            }
        }

        *self.state.write().await = ConnectionState::Disconnected;
        Ok(())
    }

    /// Stop the client.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Connect once and process events until disconnection.
    async fn connect_once(&self) -> Result<(), SseClientError> {
        *self.state.write().await = ConnectionState::Connecting;

        let response = self
            .http
            .get(self.config.connect_url())
            .header("X-Instance-Id", &self.config.instance_id)
            .header("X-Instance-Secret", &self.config.secret)
            .header("X-Repo-Identifier", &self.config.repo_identifier)
            .header(
                "X-Instance-TTL-Remaining",
                self.config.ttl_remaining.to_string(),
            )
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .send()
            .await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let body = response.text().await.unwrap_or_default();
            return Err(SseClientError::AuthenticationRejected(body));
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SseClientError::ConnectionFailed(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        *self.state.write().await = ConnectionState::Connected;
        info!(instance_id = %self.config.instance_id, "SSE connected");

        // Process event stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let heartbeat_timeout = Duration::from_secs(self.config.heartbeat_timeout_secs);

        loop {
            let chunk = tokio::select! {
                chunk = time::timeout(heartbeat_timeout, stream.next()) => {
                    match chunk {
                        Ok(Some(Ok(bytes))) => bytes,
                        Ok(Some(Err(e))) => return Err(SseClientError::HttpError(e)),
                        Ok(None) => {
                            info!("SSE stream ended");
                            return Ok(());
                        }
                        Err(_) => {
                            warn!("Heartbeat timeout, reconnecting");
                            return Err(SseClientError::ConnectionFailed("heartbeat timeout".to_string()));
                        }
                    }
                }
            };

            if !self.running.load(Ordering::Relaxed) {
                return Err(SseClientError::Stopped);
            }

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE messages (separated by \n\n)
            while let Some(pos) = buffer.find("\n\n") {
                let message = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();
                self.process_message(&message).await?;
            }
        }
    }

    /// Process a single SSE message.
    async fn process_message(&self, message: &str) -> Result<(), SseClientError> {
        for line in message.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                let event: SseEvent = serde_json::from_str(data).map_err(|e| {
                    SseClientError::EventParseError(format!("{}: {}", e, data))
                })?;

                debug!(event_type = event.event_type(), "SSE event received");

                // Handle special events
                match &event {
                    SseEvent::TtlExpired => {
                        return Err(SseClientError::TtlExpired {
                            message: "server reports TTL expired".to_string(),
                        });
                    }
                    SseEvent::Disconnected { reason } => {
                        return Err(SseClientError::ConnectionClosed {
                            reason: reason.clone(),
                        });
                    }
                    _ => {}
                }

                // Dispatch to handlers
                self.handlers.dispatch(&event).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let config = SseClientConfig::new(
            "http://localhost:8090",
            "test-instance",
            "secret",
            "default",
            3600,
        );
        let client = SseClient::new(config);
        assert!(!client.is_running());
    }

    #[tokio::test]
    async fn stop_sets_flag() {
        let config = SseClientConfig::new(
            "http://localhost:8090",
            "test",
            "secret",
            "default",
            3600,
        );
        let client = SseClient::new(config);
        client.stop();
        assert!(!client.is_running());
    }

    #[tokio::test]
    async fn initial_state_is_disconnected() {
        let config = SseClientConfig::new(
            "http://localhost:8090",
            "test",
            "secret",
            "default",
            3600,
        );
        let client = SseClient::new(config);
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }
}
