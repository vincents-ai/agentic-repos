use crate::config::Config;
use std::sync::Arc;

pub struct Server {
    config: Arc<Config>,
}

impl Server {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!(
            "Server starting on {}:{}",
            self.config.server.host, self.config.server.port
        );

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}
