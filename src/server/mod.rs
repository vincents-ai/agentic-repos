use crate::config::AppConfig;
use std::sync::Arc;

pub struct Server {
    config: Arc<AppConfig>,
}

impl Server {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!(
            "Server starting on {}:{}",
            self.config.server.host, self.config.server.port
        );

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}

pub async fn run_server(config: AppConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = Server::new(config);
    server.run().await
}
