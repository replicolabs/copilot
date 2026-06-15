use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "copilot", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run {
        #[arg(short, long, default_value_t = 5)]
        count: u32,
        #[arg(short, long)]
        tip: Option<u64>,
    },
    Inject,
    Watch,
    Logs {
        #[arg(short, long, default_value = "logs")]
        dir: String,
    },
    Status {
        #[arg(short, long)]
        bundle: String,
    },
    Keygen {
        #[arg(short, long, default_value = "copilot-keypair.json")]
        outfile: String,
        #[arg(short, long, default_value_t = false)]
        force: bool,
    },
}
