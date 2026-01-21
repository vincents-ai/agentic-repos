use repos_mcp::{config::AppConfig, server::run_server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = AppConfig::from_env()?;

    run_server(config).await?;

    Ok(())
}
