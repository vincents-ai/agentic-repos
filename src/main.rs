use agentic_repos::config::AppConfig;

/// agentic-repos — open-core AGPL git server.
///
/// Usage:
///   agentic-repos                        — start the server
///   agentic-repos init [OPTIONS]         — bootstrap a fresh installation
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialise tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("init") => run_init(&args[2..])?,
        Some("help") | Some("--help") | Some("-h") => print_help(),
        Some(cmd) => {
            eprintln!("Unknown subcommand: {}", cmd);
            eprintln!("Run 'agentic-repos help' for usage.");
            std::process::exit(1);
        }
        None => run_server().await?,
    }

    Ok(())
}

fn print_help() {
    eprintln!("agentic-repos — open-core AGPL git server\n");
    eprintln!("USAGE:");
    eprintln!("  agentic-repos                        Start the git server");
    eprintln!("  agentic-repos init [OPTIONS]         Bootstrap a fresh installation");
    eprintln!("\nINIT OPTIONS:");
    eprintln!("  --storage-root <PATH>    Storage root directory (default: ./repositories)");
    eprintln!("  --admin-key <KEY>        SSH public key to register as admin");
    eprintln!("  --ssh-port <PORT>        SSH port (default: 2222)");
    eprintln!("  --http-port <PORT>       HTTP port (default: 8090)");
}

fn run_init(args: &[String]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use agentic_repos::bootstrap::{bootstrap_system_repo, register_admin_key};

    let mut storage_root = std::path::PathBuf::from("./repositories");
    let mut admin_key: Option<String> = None;
    let mut ssh_port: Option<u16> = None;
    let mut http_port: Option<u16> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--storage-root" => {
                i += 1;
                storage_root = std::path::PathBuf::from(args.get(i).ok_or("--storage-root requires a path")?);
            }
            "--admin-key" => {
                i += 1;
                admin_key = Some(args.get(i).ok_or("--admin-key requires a key")?.clone());
            }
            "--ssh-port" => {
                i += 1;
                ssh_port = Some(args.get(i).ok_or("--ssh-port requires a number")?.parse()?);
            }
            "--http-port" => {
                i += 1;
                http_port = Some(args.get(i).ok_or("--http-port requires a number")?.parse()?);
            }
            other => {
                eprintln!("Unknown option: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    eprintln!("Bootstrapping agentic-repos...");
    eprintln!("  Storage root: {}", storage_root.display());

    // 1. Bootstrap the system repo (_adp/system)
    bootstrap_system_repo(&storage_root)?;
    eprintln!("  ✓ System repo bootstrapped");

    // 2. Override ports if specified
    if ssh_port.is_some() || http_port.is_some() {
        let system_repo = storage_root.join("_adp/system");
        let existing: serde_json::Value = agentic_repos::bootstrap::read_ref_json(
            &system_repo,
            "refs/adp/config/server",
        )?.unwrap_or_default();

        let mut config = existing;
        if let Some(p) = http_port {
            config["http_port"] = serde_json::json!(p);
        }
        if let Some(p) = ssh_port {
            config["ssh_port"] = serde_json::json!(p);
        }
        agentic_repos::bootstrap::write_ref_json(
            &system_repo,
            "refs/adp/config/server",
            &config,
        )?;
        eprintln!("  ✓ Config updated");
    }

    // 3. Register admin key if provided
    if let Some(key) = &admin_key {
        register_admin_key(&storage_root, "admin", key)?;
        eprintln!("  ✓ Admin key registered");
    }

    eprintln!("\nBootstrap complete. Run 'agentic-repos' to start the server.");
    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = AppConfig::from_env()?;
    agentic_repos::server::run_server(config).await?;
    Ok(())
}
