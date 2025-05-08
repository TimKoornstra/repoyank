mod cli;
mod clipboard;
mod file_scanner;
mod tree_builder;
mod utils;

use anyhow::Result;
use clap::Parser;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use ignore::WalkBuilder as IgnoreWalkBuilder;
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    // Handle daemon mode first.
    if clipboard::check_and_run_daemon_if_requested()? {
        return Ok(());
    }

    let cli_args = cli::Cli::parse();

    // 1. Scan all potential files and directories based on CLI args
    let all_discovered_items =
        file_scanner::scan_files(&cli_args.root, &cli_args.types, cli_args.include_ignored)?;

    if all_discovered_items.is_empty()
        || (all_discovered_items.len() == 1
            && all_discovered_items[0].0 == cli_args.root
            && all_discovered_items[0].1)
    {
        println!("No matching files or non-empty directories found to select from.");
        return Ok(());
    }

    // 2. Build a provisional tree
    let display_labels = tree_builder::build_tree_labels(&all_discovered_items, &cli_args.root);

    if display_labels.is_empty() || (display_labels.len() == 1 && display_labels[0] == "./") {
        println!(
            "No items to display for selection after filtering (or only root './' which is implicitly included)."
        );
        return Ok(());
    }

    // 3. Prompt the user for selections
    let selections_indices = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select files or directories (Space to toggle, Enter to confirm)")
        .items(&display_labels)
        .interact()?;

    if selections_indices.is_empty() {
        println!("No items selected. Exiting.");
        return Ok(());
    }

    // 4. Determine the actual files to include based on selections
    let mut picked_files_content: Vec<PathBuf> = Vec::new();
    for &selected_idx in &selections_indices {
        let (selected_path, is_dir) = &all_discovered_items[selected_idx];
        if *is_dir {
            let mut dir_walker = IgnoreWalkBuilder::new(selected_path);
            if cli_args.include_ignored {
                dir_walker.git_ignore(false).ignore(false);
            }
            for entry_result in dir_walker.build() {
                if let Ok(entry) = entry_result {
                    if entry.file_type().map_or(false, |ft| ft.is_file()) {
                        let path = entry.into_path();
                        if !cli_args.types.is_empty() {
                            let keep = cli_args
                                .types
                                .iter()
                                .any(|ext| path.extension() == Some(ext.as_os_str()));
                            if keep {
                                picked_files_content.push(path);
                            }
                        } else {
                            picked_files_content.push(path);
                        }
                    }
                }
            }
        } else {
            picked_files_content.push(selected_path.clone());
        }
    }
    picked_files_content.sort();
    picked_files_content.dedup();

    if picked_files_content.is_empty() {
        println!("No actual files to copy after expanding selections. Exiting.");
        return Ok(());
    }

    // 5. Construct the list of nodes for the output tree display
    let mut final_tree_nodes: Vec<(PathBuf, bool)> = Vec::new();
    if !all_discovered_items.is_empty() && all_discovered_items[0].0 == cli_args.root {
        final_tree_nodes.push(all_discovered_items[0].clone());
    } else if cli_args.root.exists() {
        final_tree_nodes.push((cli_args.root.clone(), true));
    }

    for (path, is_dir) in &all_discovered_items {
        let directly_selected = selections_indices
            .iter()
            .any(|&idx| all_discovered_items[idx].0 == *path);
        let descendant_of_selected_dir = selections_indices.iter().any(|&idx| {
            let (sel_path, sel_is_dir) = &all_discovered_items[idx];
            *sel_is_dir && path.starts_with(sel_path) && path != sel_path
        });

        if directly_selected || descendant_of_selected_dir {
            final_tree_nodes.push((path.clone(), *is_dir));
            let mut current = path.clone();
            while let Some(parent) = current.parent() {
                if parent == cli_args.root && !final_tree_nodes.iter().any(|(p, _)| p == parent) {
                    if let Some(root_item) = all_discovered_items
                        .iter()
                        .find(|(p, _)| p == &cli_args.root)
                    {
                        final_tree_nodes.push(root_item.clone());
                    } else {
                        final_tree_nodes.push((parent.to_path_buf(), true));
                    }
                    break;
                }
                if parent.starts_with(&cli_args.root) && parent != &cli_args.root {
                    if !final_tree_nodes.iter().any(|(p, _)| p == parent) {
                        let parent_is_dir = all_discovered_items
                            .iter()
                            .find(|(p, _)| p == parent)
                            .map_or(true, |(_, is_d)| *is_d);
                        final_tree_nodes.push((parent.to_path_buf(), parent_is_dir));
                    }
                } else {
                    break;
                }
                current = parent.to_path_buf();
            }
        }
    }
    final_tree_nodes.sort_by(|(a, _), (b, _)| a.cmp(b));
    final_tree_nodes.dedup_by(|(a, _), (b, _)| a == b);

    // 6. Build the output tree
    let output_tree_labels = tree_builder::build_tree_labels(&final_tree_nodes, &cli_args.root);
    let mut output_string = String::new();
    for label in output_tree_labels {
        output_string.push_str(&label);
        output_string.push('\n');
    }
    output_string.push('\n');

    // 7. Append file contents
    for file_path in &picked_files_content {
        let relative_path = file_path.strip_prefix(&cli_args.root).unwrap_or(file_path);
        match fs::read_to_string(file_path) {
            Ok(contents) => {
                output_string.push_str(&format!(
                    "---\nFile: {}\n---\n\n{}\n\n",
                    relative_path.display(),
                    contents.trim_end()
                ));
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Warning: Could not read file {}: {}",
                    file_path.display(),
                    e
                );
                output_string.push_str(&format!(
                    "---\nFile: {} (Error reading file: {})\n---\n\n[Content not available]\n\n",
                    relative_path.display(),
                    e
                ));
            }
        }
    }
    let final_output_string = output_string.trim_end_matches('\n').to_string() + "\n";

    // 8. Copy to clipboard and print summary
    let tokens = utils::approx_tokens(&final_output_string);
    clipboard::copy_text_to_clipboard(final_output_string)?;

    println!(
        "✅ Copied {} files (≈ {} tokens) to the clipboard.",
        picked_files_content.len(),
        tokens
    );

    Ok(())
}
