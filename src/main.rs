mod agents;
mod cli;
mod commands;
mod config;
mod cow;
mod error;
mod fs_atomic;
mod git;
mod hooks;
mod post_create;
mod process;
mod release;
mod rewrite;
mod terminal;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    // Resolve --debug before initializing the logger; env_logger captures the
    // filter at init time, so flipping RUST_LOG inside Cli::run is too late.
    let want_debug = std::env::args().any(|arg| arg == "--debug" || arg == "-d");
    let mut builder = env_logger::Builder::from_default_env();
    if want_debug && std::env::var_os("RUST_LOG").is_none() {
        builder.filter_level(log::LevelFilter::Debug);
    }
    builder.init();
    log::debug!("Debug logging enabled");

    let cli = Cli::parse();
    cli.run()
}
