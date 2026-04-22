mod cli;
mod config;
mod cow;
mod error;
mod git;
mod hooks;
mod process;
mod rewrite;
mod terminal;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle the command
    cli.run()
}
