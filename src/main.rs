use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "psknife",
    version,
    about = "TUI toolbox to maintain PowerShell"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List backups and restore one (defaults to the latest).
    Restore {
        #[arg(long)]
        timestamp: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Restore { timestamp }) => {
            powershellknife::backup::restore(timestamp.as_deref())
        }
        None => powershellknife::app::run(),
    }
}
