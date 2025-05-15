mod cli;
mod clipboard;
mod file_scanner;
mod tree_builder;
mod tui;
mod utils;
mod workflow;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    // Handle daemon mode first. This should stay in main.rs as it's an early exit.
    if clipboard::check_and_run_daemon_if_requested()? {
        return Ok(());
    }

    let cli_args = cli::Cli::parse();

    // Delegate the main application logic to the workflow module
    workflow::run_repoyank(cli_args)
}
