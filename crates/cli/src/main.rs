mod banner;
mod cli;
mod commands;
mod config;
mod pipeline;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    init_tracing();

    let cli = Cli::parse();
    banner::print_banner();

    match cli.command {
        Command::Run { count, tip } => commands::run(Config::from_env()?, count, tip).await,
        Command::Inject => commands::inject(Config::from_env()?).await,
        Command::Watch => commands::watch(Config::from_env()?).await,
        Command::Logs { dir } => commands::logs(&dir),
        Command::Status { bundle } => commands::status(Config::from_env()?, &bundle).await,
        Command::Keygen { outfile, force } => commands::keygen(&outfile, force),
    }
}

fn load_dotenv() {
    let _ = dotenvy::dotenv();

    let home = std::env::var("COPILOT_HOME").ok().unwrap_or_else(|| {
        std::env::var("HOME")
            .map(|h| format!("{h}/.copilot"))
            .unwrap_or_default()
    });
    if !home.is_empty() {
        let _ = dotenvy::from_path(std::path::Path::new(&home).join(".env"));
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_env("COPILOT_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
