use ogit::{config::AppConfig, server::run_server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = AppConfig::from_env().expect("Failed to load configuration");

    run_server(config)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}
