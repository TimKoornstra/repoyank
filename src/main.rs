mod cli;
mod clipboard;
mod file_scanner;
mod tree_builder;
mod tui;
mod utils;

use anyhow::Result;
use clap::Parser;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

fn main() -> Result<()> {
    // Handle daemon mode first.
    if clipboard::check_and_run_daemon_if_requested()? {
        return Ok(());
    }

    let cli_args = cli::Cli::parse();

    // 1. Scan all potential files and directories based on CLI args
    let initial_scan_results =
        file_scanner::scan_files(&cli_args.root, &cli_args.types, cli_args.include_ignored)?;

    // Filter out directories that are effectively empty after type filtering for the selection prompt
    let mut paths_for_selection_prompt_set = HashSet::new();

    for (path, is_dir) in &initial_scan_results {
        if !*is_dir {
            paths_for_selection_prompt_set.insert(path.clone());
            let mut current_ancestor = path.parent();
            while let Some(ancestor_path) = current_ancestor {
                if ancestor_path.starts_with(&cli_args.root) || ancestor_path == &cli_args.root {
                    paths_for_selection_prompt_set.insert(ancestor_path.to_path_buf());
                    if ancestor_path == &cli_args.root {
                        break;
                    }
                    current_ancestor = ancestor_path.parent();
                } else {
                    break;
                }
            }
        } else if path == &cli_args.root {
            paths_for_selection_prompt_set.insert(path.clone());
        }
    }
    if initial_scan_results
        .iter()
        .any(|(p, _)| p == &cli_args.root)
    {
        if !paths_for_selection_prompt_set.contains(&cli_args.root) && cli_args.root.is_dir() {
            // Ensure root dir is in if it exists
            paths_for_selection_prompt_set.insert(cli_args.root.clone());
        }
    }

    let initial_scan_map: HashMap<PathBuf, bool> = initial_scan_results.iter().cloned().collect();
    let mut selectable_items_for_tui: Vec<(PathBuf, bool)> = paths_for_selection_prompt_set
        .into_iter()
        .filter_map(|path| initial_scan_map.get(&path).map(|is_dir| (path, *is_dir)))
        .collect();

    // If root was not in initial_scan_map (e.g. empty dir scan returned nothing but root path)
    // ensure it's added if it exists and is a directory.
    if !selectable_items_for_tui
        .iter()
        .any(|(p, _)| p == &cli_args.root)
        && cli_args.root.exists()
        && cli_args.root.is_dir()
    {
        selectable_items_for_tui.push((cli_args.root.clone(), true));
    }

    selectable_items_for_tui.sort_by(|(a, _), (b, _)| a.cmp(b));
    selectable_items_for_tui.dedup_by(|(a, _), (b, _)| a == b);

    if selectable_items_for_tui.is_empty() {
        println!("No matching files or directories found to select from.");
        return Ok(());
    }

    let display_labels = tree_builder::build_tree_labels(&selectable_items_for_tui, &cli_args.root);

    // 3. Prompt the user for selections using the new TUI
    let tui_result_items =
        match tui::run_tui(&selectable_items_for_tui, &display_labels, &cli_args.root)? {
            Some(items) => items,
            _ => {
                println!("Selection cancelled. Exiting.");
                return Ok(());
            }
        };

    // 4. Determine the actual files to include based on TUI selections
    let mut picked_files_content: Vec<PathBuf> = tui_result_items
        .iter()
        .filter(|item| !item.is_dir && item.state == tui::SelectionState::FullySelected)
        .map(|item| item.path.clone())
        .collect();

    picked_files_content.sort();
    picked_files_content.dedup();

    if picked_files_content.is_empty() {
        println!("No files selected to copy. Exiting.");
        return Ok(());
    }

    // 5. Construct the list of nodes for the output tree display
    let mut final_tree_node_paths_set = HashSet::new();
    // Always include the root in the tree display if it exists.
    // is_dir status from initial_scan_map or direct check.
    if cli_args.root.exists() {
        final_tree_node_paths_set.insert(cli_args.root.clone());
    }

    for item in &tui_result_items {
        // Include items that are fully or partially selected in the tree
        if item.state == tui::SelectionState::FullySelected
            || item.state == tui::SelectionState::PartiallySelected
        {
            final_tree_node_paths_set.insert(item.path.clone());
            // Also include all ancestors of selected items up to the root
            let mut current_ancestor = item.path.parent();
            while let Some(ancestor_path) = current_ancestor {
                if ancestor_path.starts_with(&cli_args.root) || ancestor_path == &cli_args.root {
                    final_tree_node_paths_set.insert(ancestor_path.to_path_buf());
                    if ancestor_path == &cli_args.root {
                        break;
                    }
                    current_ancestor = ancestor_path.parent();
                } else {
                    break; // Ancestor is outside the root
                }
            }
        }
    }

    let mut final_tree_nodes: Vec<(PathBuf, bool)> = final_tree_node_paths_set
        .into_iter()
        .map(|p| {
            // Get is_dir status from initial scan, or check fs as fallback
            let is_dir = initial_scan_map
                .get(&p)
                .copied()
                .unwrap_or_else(|| p.is_dir());
            (p, is_dir)
        })
        .collect();

    final_tree_nodes.sort_by(|(a, _), (b, _)| a.cmp(b));
    // HashSet ensures deduplication

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
        // This now uses the correctly filtered list
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
