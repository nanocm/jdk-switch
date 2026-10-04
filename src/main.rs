mod cli;
mod commands;
mod config;
mod downloader;
mod env;
mod error;
mod jdk;
use clap::Parser;
use cli::{Cli, Commands};
use colored::*;
#[tokio::main]
async fn main() {
    if std::env::var("NO_COLOR").is_ok() || !supports_color() {
        control::set_override(false);
    }
    if let Err(e) = run().await {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
fn supports_color() -> bool {
    if let Ok(wt) = std::env::var("WT_SESSION") {
        return !wt.is_empty();
    }
    if let Ok(term) = std::env::var("TERM") {
        return !term.is_empty() && term != "dumb";
    }
    #[cfg(target_os = "windows")]
    {
        if std::env::var("ANSICON").is_ok() {
            return true;
        }
        return false;
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}
async fn run() -> error::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List { scan, prune } => {
            commands::list_command(scan, prune)?;
        }
        Commands::Current => {
            commands::current_command()?;
        }
        Commands::Use { version } => {
            commands::use_command(&version)?;
        }
        Commands::Download { version, vendor } => {
            commands::download_command(&version, vendor).await?;
        }
        Commands::Search { keyword, vendor } => {
            commands::search_command(keyword, vendor).await?;
        }
    }
    Ok(())
}
