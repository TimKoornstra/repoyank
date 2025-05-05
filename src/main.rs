use std::collections::HashMap;
use std::path::Component;
use std::{ffi::OsStr, fs, path::PathBuf};

use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
use clap::{Parser, ValueEnum};
use dialoguer::{theme::ColorfulTheme, MultiSelect};
use ignore::WalkBuilder;

/// Internal flag – don’t document it, just keep it unlikely to clash
const DAEMON_FLAG: &str = "__clipboard_daemon";

/// Rough estimate: GPT-style token ≈ 4 chars (good enough for UI)
fn approx_tokens(s: &str) -> usize {
    s.chars().count() / 4
}

/// A comma-separated list of file-extensions passed via --types
#[derive(Clone, Debug, ValueEnum)]
enum Ext {
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
    fn as_os_str(&self) -> &'static OsStr {
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

/// repoyank – copy annotated source snippets to clipboard
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Root to scan (defaults to CWD)
    #[arg(value_name = "DIR", default_value = ".")]
    root: PathBuf,

    /// Comma-separated file-types to include (extension only, no dot).
    #[arg(long, value_delimiter = ',')]
    types: Vec<Ext>,

    /// Include files ignored by .gitignore
    #[arg(long)]
    include_ignored: bool,
}

/// Build pretty tree-style labels in **O(n)**.
///
/// * `paths` **must** be lexicographically sorted the same way `tree` does
/// * Each element in `paths` is `(path, is_dir)`.
pub fn build_tree_labels(paths: &[(PathBuf, bool)], root: &PathBuf) -> Vec<String> {
    let n = paths.len();
    let mut labels = Vec::with_capacity(n);
    let mut is_last_vec = vec![false; n];
    let mut last_child_map = HashMap::<PathBuf, usize>::new();

    // PASS #1 – record each directory’s last immediate child index
    for (idx, (path, _)) in paths.iter().enumerate() {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let parent = rel.parent().unwrap_or(std::path::Path::new(""));
        last_child_map.insert(parent.to_path_buf(), idx);
    }

    // PASS #2 – scan once, using a stack to track ancestors
    let mut ancestor_stack: Vec<usize> = Vec::new();
    for (idx, (path, is_dir)) in paths.iter().enumerate() {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let depth = rel.components().count();

        while ancestor_stack.len() > depth {
            ancestor_stack.pop();
        }

        let parent = rel.parent().unwrap_or(std::path::Path::new(""));
        let is_last = last_child_map[&parent.to_path_buf()] == idx;
        is_last_vec[idx] = is_last;

        // Build indentation only if we’re *below* the root.
        let mut prefix = String::new();
        if depth >= 1 {
            // skip the root-level ancestor so direct children don't get a leading "│  "
            if depth > 1 {
                for &anc in &ancestor_stack[1..] {
                    prefix.push_str(if is_last_vec[anc] { "   " } else { "│  " });
                }
            }
            // now draw the branch for *this* node
            prefix.push_str(if is_last { "└─ " } else { "├─ " });
        }

        let name = rel
            .components()
            .last()
            .unwrap_or(Component::CurDir)
            .as_os_str()
            .to_string_lossy();
        // Special-case the project root so it prints as “./”
        let label = if depth == 0 {
            "./".to_string()
        } else if *is_dir {
            format!("{}{}/", prefix, name)
        } else {
            format!("{}{}", prefix, name)
        };
        labels.push(label);
        ancestor_stack.push(idx);
    }

    labels
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ──────────────────────────────────────────────────────────────
    // If we *are* the daemon, read stdin, set clipboard, and stay alive
    // ──────────────────────────────────────────────────────────────
    #[cfg(target_os = "linux")]
    if std::env::args().any(|a| a == DAEMON_FLAG) {
        // Read all of stdin as the clipboard text
        let text = std::io::read_to_string(std::io::stdin())?;

        // Claim ownership and provide the data, keeping the Waiter alive
        let _waiter = Clipboard::new()? // 1. open clipboard
            .set() // 2. request ownership
            .wait() // 3. wait until we have it
            .text(text)?; // 4. supply the clipboard bytes

        // Keep the process alive so the clipboard stays valid
        std::thread::park();
        unreachable!();
    }

    // Build the file tree ------------------------------------------------
    let mut tree: Vec<(PathBuf, bool)> = Vec::new();
    let mut walker = WalkBuilder::new(&cli.root);
    if cli.include_ignored {
        walker.git_ignore(false).ignore(false);
    }
    for result in walker.build() {
        let dirent = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠️  {e}");
                continue;
            }
        };

        if !cli.types.is_empty() && dirent.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let keep = cli
                .types
                .iter()
                .any(|ext| dirent.path().extension() == Some(ext.as_os_str()));
            if !keep {
                continue;
            }
        }

        let is_dir = dirent.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let path = dirent.into_path();
        tree.push((path, is_dir));
    }
    tree.sort();

    // Generate display labels
    let items = build_tree_labels(&tree, &cli.root);

    // Prompt the user -----------------------------------------------------
    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select files or directories (space to toggle, ⏎ to confirm)")
        .items(&items)
        .interact()?;

    if selections.is_empty() {
        println!("No files selected – exiting.");
        return Ok(());
    }

    let selections_clone = selections.clone();

    // Expand directories into picked_files (unchanged)
    let mut picked_files = Vec::<PathBuf>::new();
    for idx in &selections_clone {
        let (sel_path, sel_is_dir) = &tree[*idx];
        if *sel_is_dir {
            for entry in WalkBuilder::new(sel_path).build() {
                if let Ok(ent) = entry {
                    if ent.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        picked_files.push(ent.into_path());
                    }
                }
            }
        } else {
            picked_files.push(sel_path.clone());
        }
    }
    picked_files.sort();
    picked_files.dedup();

    if picked_files.is_empty() {
        println!("Nothing to copy.");
        return Ok(());
    }

    // 1. Rebuild the tree labels to include every descendant of a selected dir
    let mut chosen_nodes: Vec<(PathBuf, bool)> = tree
        .iter()
        .filter(|(path, _)| {
            selections_clone.iter().any(|&idx| {
                let (sel_path, sel_is_dir) = &tree[idx];
                // if the selection was a directory, include all paths under it,
                // otherwise include only the exact file
                if *sel_is_dir {
                    path.starts_with(sel_path)
                } else {
                    path == sel_path
                }
            })
        })
        .cloned()
        .collect();

    // ensure the root itself is shown at depth 0
    if !chosen_nodes.iter().any(|(p, _)| p == &cli.root) {
        chosen_nodes.push((cli.root.clone(), true));
    }

    // include all ancestors of every chosen node, so intermediate dirs show up
    let mut extra = Vec::new();
    for (path, _) in &chosen_nodes {
        // walk up from `path` to `cli.root`
        let mut cur = path.parent();
        while let Some(parent) = cur {
            if parent.starts_with(&cli.root)
                && !chosen_nodes.iter().any(|(p, _)| p == parent)
                && !extra.iter().any(|(p, _)| p == parent)
            {
                // look up in `tree` to see if it's really a dir (it should be)
                let is_dir = tree
                    .iter()
                    .find(|(p2, _)| p2 == parent)
                    .map(|(_, d)| *d)
                    .unwrap_or(true);
                extra.push((parent.to_path_buf(), is_dir));
            }
            cur = parent.parent();
        }
    }
    chosen_nodes.extend(extra);
    chosen_nodes.sort();

    let chosen_tree = build_tree_labels(&chosen_nodes, &cli.root);

    let mut output = String::new();
    for line in chosen_tree {
        output.push_str(&line);
        output.push('\n');
    }
    output.push('\n'); // blank line before the file bodies

    // ────────────────────────────────────────────────────────────────────
    // 2. Concatenate the contents (existing behaviour)
    // ────────────────────────────────────────────────────────────────────
    for file in &picked_files {
        let rel = file.strip_prefix(&cli.root).unwrap_or(file);
        let contents = fs::read_to_string(file)?;
        output.push_str(&format!(
            "---\nFile: {}\n---\n\n{}\n\n",
            rel.display(),
            contents
        ));
    }

    // Append token estimate
    let tokens = approx_tokens(&output);

    // Spawn daemon or set directly ---------------------------------------
    #[cfg(not(target_os = "linux"))]
    Clipboard::new()?.set_text(output.clone())?;

    #[cfg(target_os = "linux")]
    {
        // Launch the daemon helper to hold the clipboard
        use std::process::{Command, Stdio};
        let mut child = Command::new(std::env::current_exe()?)
            .arg(DAEMON_FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .current_dir("/")
            .spawn()?;
        use std::io::Write;
        child.stdin.as_mut().unwrap().write_all(output.as_bytes())?;
    }

    println!(
        "✅ Copied {} files (≈ {} tokens) to the clipboard.",
        picked_files.len(),
        tokens
    );
    Ok(())
}
