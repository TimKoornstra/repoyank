use clap::{Parser, ValueEnum};
use std::ffi::OsStr;
use std::path::PathBuf;

/// repoyank – copy annotated source snippets to clipboard
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Root to scan (defaults to CWD)
    #[arg(value_name = "DIR", default_value = ".")]
    pub root: PathBuf,

    /// Comma-separated file-types to include (extension only, no dot).
    #[arg(long, value_delimiter = ',')]
    pub types: Vec<Ext>,

    /// Include files ignored by .gitignore
    #[arg(long)]
    pub include_ignored: bool,
}

/// A comma-separated list of file-extensions passed via --types
#[derive(Clone, Debug, ValueEnum)]
pub enum Ext {
    Rs,
    Md,
    Ex,
    Exs,
    Txt,
    Json,
    Yaml,
    Toml,
    // add more as needed
}

impl Ext {
    pub fn as_os_str(&self) -> &'static OsStr {
        match self {
            Ext::Rs => OsStr::new("rs"),
            Ext::Md => OsStr::new("md"),
            Ext::Ex => OsStr::new("ex"),
            Ext::Exs => OsStr::new("exs"),
            Ext::Txt => OsStr::new("txt"),
            Ext::Json => OsStr::new("json"),
            Ext::Yaml => OsStr::new("yaml"),
            Ext::Toml => OsStr::new("toml"),
        }
    }
}
