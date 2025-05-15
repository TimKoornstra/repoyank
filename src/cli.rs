use clap::Parser;
use std::path::PathBuf;

/// repoyank – copy annotated source snippets to clipboard
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Root to scan (defaults to CWD)
    #[arg(value_name = "DIR", default_value = ".")]
    pub root: PathBuf,

    /// Comma-separated file-types to include (extension only, no dot).
    #[arg(long, value_delimiter = ',', value_name = "EXTENSIONS")]
    pub types: Vec<String>,

    /// Include files ignored by .gitignore
    #[arg(long)]
    pub include_ignored: bool,

    // Glob patterns to preselect files (e.g., "src/**/*.rs", "tests/test_*.py").
    /// Paths are relative to the root directory.
    /// Can be specified multiple times using --preselect <PATTERN_1> --preselect <PATTERN_2> ...
    #[arg(long, value_name = "PATTERN")]
    pub preselect: Vec<String>,

    /// Run in headless mode: select files based on --preselect and exit without TUI.
    /// Requires --preselect to be specified.
    #[arg(long)]
    pub headless: bool,
}
