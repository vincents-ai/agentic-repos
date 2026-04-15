use agentic_repos::{adp::bootstrap_system_repo, config::AppConfig, server::run_server};
use std::path::PathBuf;

/// Simple subcommand handling without pulling in a CLI framework dependency.
///
/// Usage:
///   agentic-repos              — start the server (default)
///   agentic-repos init          — bootstrap the system repo and exit
///   agentic-repos init --storage <path>  — bootstrap at a custom storage root
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();

    // Check for the `init` subcommand as the first non-binary argument.
    if args.get(1).map(|s| s.as_str()) == Some("init") {
        // Determine storage root: --storage <path> or default "./repositories"
        let storage_root =
            parse_storage_flag(&args).unwrap_or_else(|| PathBuf::from("./repositories"));

        // Initialise tracing so bootstrap log messages are visible
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .init();

        bootstrap_system_repo(&storage_root)?;
        return Ok(());
    }

    // Initialise tracing for the normal server start path
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    run_server(config).await?;

    Ok(())
}

/// Parse `--storage <path>` from the argument list, returning `None` if the
/// flag is not present.
fn parse_storage_flag(args: &[String]) -> Option<PathBuf> {
    args.windows(2).find_map(|pair| {
        if pair[0] == "--storage" {
            Some(PathBuf::from(&pair[1]))
        } else {
            None
        }
    })
}
