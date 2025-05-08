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
}
