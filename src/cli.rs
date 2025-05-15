use clap::Parser;

fn command_long_about() -> String {
    format!(
        "repoyank v{} - Interactively select and copy code snippets.

Repoyank helps you build context for LLMs by selecting files and directories
from a repository, formatting them, and copying them to your clipboard.

USAGE:
    repoyank [OPTIONS] [PATTERN ...]

ARGUMENTS:
    [PATTERN ...]
        Zero or more shell-style globs (e.g., 'src/**/*.rs', 'docs/*.md').
        Globs are resolved relative to the scan root.
        If the first PATTERN provided is an existing directory, it is used as the
        scan root. Otherwise, the current working directory is the scan root.
        If no patterns are given, it defaults to selecting all files ('**/*')
        under the scan root.

OPTIONS (see `repoyank --help` for full details):
    -a, --all                 Skip TUI, yank all files matching patterns & filters.
    -t, --type <EXT[,EXT...]> Filter by file extensions (e.g., rs,md).
    -s, --select <GLOB[,...]> Pre-select TUI items matching these globs.
    -i, --include-ignored     Include files ignored by .gitignore.
    -n, --dry-run             Print selection and tree, but don't copy to clipboard.
    -h, --help                Show help.
    -V, --version             Show version.

EXAMPLES:
    repoyank                          # Browse repo and cherry-pick
    repoyank -t py                    # Show only Python files, pick interactively
    repoyank -s 'tests/**/*.cpp'      # Pre-highlight test cpp files in TUI
    repoyank -a 'tests/**/*.cpp'      # Instantly yank exactly the test cpp files
    repoyank -a -t rs,md              # Yank all Rust & MD files, no TUI
    repoyank -n -a docs/**/*.md       # See what would be yanked (dry run)
",
        env!("CARGO_PKG_VERSION")
    )
}

/// repoyank – copy annotated source snippets to clipboard
#[derive(Parser, Debug)]
#[command(author, version, about = "Interactively select and copy code snippets.", long_about = command_long_about())]
pub struct Cli {
    /// Globs to select files/directories. First dir PATTERN sets scan root.
    /// Defaults to '**/*' if no patterns.
    #[arg(value_name = "PATTERN")]
    pub patterns: Vec<String>,

    /// Skip TUI, yank all files matching patterns & filters.
    #[arg(short = 'a', long, alias = "headless")]
    pub all: bool,

    /// Filter by comma-separated file extensions (e.g., rs,md; no dots).
    #[arg(
        short = 't',
        long = "type",
        value_delimiter = ',',
        value_name = "EXT",
        alias = "types"
    )]
    pub type_filter: Vec<String>,

    /// Pre-select TUI items matching these comma-separated globs.
    /// Globs are relative to the scan root.
    #[arg(
        short = 's',
        long = "select",
        value_delimiter = ',',
        value_name = "GLOB",
        alias = "preselect"
    )]
    pub select_globs: Vec<String>,

    /// Include files ignored by .gitignore.
    #[arg(short = 'i', long)]
    pub include_ignored: bool,

    /// Print selection and tree, but don't copy to clipboard.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}
