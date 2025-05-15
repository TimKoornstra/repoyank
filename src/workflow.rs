use crate::{cli, clipboard, file_scanner, tree_builder, tui, utils};
use anyhow::Result;
use glob::Pattern;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

// Helper function to determine the effective root directory for scanning and the glob patterns to apply.
// Handles CLI arguments for patterns and deriving the scan root.
fn determine_scan_configuration(cli_args: &cli::Cli) -> Result<(PathBuf, Vec<Pattern>)> {
    let mut scan_root = PathBuf::from("."); // Default to Current Working Directory
    let mut actual_patterns_str: Vec<String> = cli_args.patterns.clone();

    // If the first positional argument is a directory, use it as the scan_root.
    if let Some(first_pattern_str) = cli_args.patterns.get(0) {
        let potential_root_path = PathBuf::from(first_pattern_str);
        if potential_root_path.is_dir() {
            scan_root = potential_root_path
                .canonicalize()
                .unwrap_or_else(|_| potential_root_path.clone());
            // Remaining positional arguments are the patterns.
            actual_patterns_str = cli_args.patterns.get(1..).unwrap_or_default().to_vec();
        }
    }

    // If no patterns are left (or none were provided initially aside from a possible root), default to "**/*".
    if actual_patterns_str.is_empty() {
        actual_patterns_str.push("**/*".to_string());
    }

    // Compile string patterns into glob::Pattern objects.
    let glob_filter_patterns: Vec<Pattern> = actual_patterns_str
        .iter()
        .filter_map(|s| match Pattern::new(s) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("⚠️ Warning: Invalid PATTERN '{}': {}", s, e);
                None
            }
        })
        .collect();

    Ok((scan_root, glob_filter_patterns))
}

// Scans for files and directories based on scan_root and applies --type filter,
// then further filters based on the primary glob patterns.
fn gather_initial_candidates(
    scan_root: &Path,
    type_filter: &[String],
    include_ignored: bool,
    glob_filter_patterns: &[Pattern],
) -> Result<Vec<(PathBuf, bool)>> {
    // Initial broad scan respecting --type and --include-ignored.
    let all_found_items_from_scan =
        file_scanner::scan_files(scan_root, type_filter, include_ignored)?;

    // Filter the broad scan results using the primary glob patterns.
    let mut initial_scan_results: Vec<(PathBuf, bool)> = all_found_items_from_scan
        .into_iter()
        .filter(|(path, is_dir)| {
            if *is_dir {
                // Directories are kept for now; their relevance is determined later.
                true
            } else {
                // For files, check if they match any of the glob patterns relative to scan_root.
                if let Ok(relative_path) = path.strip_prefix(scan_root) {
                    let path_to_match = if relative_path.as_os_str().is_empty() {
                        // File is scan_root
                        scan_root
                            .file_name()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| relative_path.to_path_buf())
                    } else {
                        relative_path.to_path_buf()
                    };
                    glob_filter_patterns
                        .iter()
                        .any(|p| p.matches_path(&path_to_match))
                } else {
                    false // Path not under scan_root, should not occur.
                }
            }
        })
        .collect();

    // Ensure scan_root itself is included in results if it's a directory and relevant.
    if !initial_scan_results.iter().any(|(p, _)| p == scan_root) && scan_root.is_dir() {
        let root_explicitly_matched_or_implied = glob_filter_patterns.iter().any(|p| {
            p.matches_path(Path::new("."))
                || p.as_str() == "**/*"
                || (scan_root
                    .file_name()
                    .map_or(false, |name| p.matches_path(Path::new(name))))
        });
        let has_children_in_results = initial_scan_results
            .iter()
            .any(|(p, _)| p.starts_with(scan_root) && p != scan_root);

        if root_explicitly_matched_or_implied || has_children_in_results {
            initial_scan_results.push((scan_root.to_path_buf(), true));
        }
    }

    // Sort for consistent processing and display.
    initial_scan_results.sort_by(|(a, _), (b, _)| a.cmp(b));
    initial_scan_results.dedup_by(|(a, _), (b, _)| a == b);

    Ok(initial_scan_results)
}

// Handles the --all (headless) mode: directly selects files and prepares data for output.
fn run_headless_mode(
    initial_scan_results: &[(PathBuf, bool)], // Already filtered candidates
    scan_root: &Path,
) -> Result<(Vec<tui::SelectableItem>, Vec<PathBuf>)> {
    // All non-directory items from the candidates are considered for yanking.
    let mut files_to_yank: Vec<PathBuf> = initial_scan_results
        .iter()
        .filter(|(_, is_dir)| !*is_dir)
        .map(|(path, _)| path.clone())
        .collect();

    files_to_yank.sort();
    files_to_yank.dedup();

    // Prepare a list of items (like TUI items) for building the output tree.
    // This includes yanked files and their ancestor directories up to scan_root.
    let mut items_for_tree_building_set = HashSet::new();
    if scan_root.exists() && scan_root.is_dir() {
        items_for_tree_building_set.insert(scan_root.to_path_buf());
    }

    for file_path in &files_to_yank {
        items_for_tree_building_set.insert(file_path.clone());
        let mut current_ancestor = file_path.parent();
        while let Some(ancestor_path) = current_ancestor {
            if ancestor_path.starts_with(scan_root) || ancestor_path == scan_root {
                items_for_tree_building_set.insert(ancestor_path.to_path_buf());
                if ancestor_path == scan_root {
                    break;
                }
                current_ancestor = ancestor_path.parent();
            } else {
                break;
            }
        }
    }

    let path_to_is_dir_map: HashMap<PathBuf, bool> = initial_scan_results.iter().cloned().collect();

    let mut temp_final_items_for_tree: Vec<(PathBuf, bool)> = items_for_tree_building_set
        .into_iter()
        .map(|p| {
            (
                p.clone(),
                path_to_is_dir_map
                    .get(&p)
                    .copied()
                    .unwrap_or_else(|| p.is_dir()),
            )
        })
        .collect();
    temp_final_items_for_tree.sort_by(|(a, _), (b, _)| a.cmp(b));

    // Create SelectableItem structs for tree generation.
    let final_tui_items_for_tree = temp_final_items_for_tree
        .iter()
        .map(|(path, is_dir)| tui::SelectableItem {
            path: path.clone(),
            display_text: "".to_string(),
            is_dir: *is_dir,
            is_expanded: true,
            state: if !*is_dir && files_to_yank.contains(path) {
                tui::SelectionState::FullySelected
            } else if *is_dir {
                tui::SelectionState::PartiallySelected
            } else {
                tui::SelectionState::NotSelected
            },
            children_indices: vec![],
            parent_index: None,
        })
        .collect();

    Ok((final_tui_items_for_tree, files_to_yank))
}

// Handles interactive TUI mode: prepares data for TUI, runs TUI, processes selections.
fn run_interactive_mode(
    initial_scan_results: &[(PathBuf, bool)],
    cli_args: &cli::Cli,
    scan_root: &Path,
) -> Result<Option<(Vec<tui::SelectableItem>, Vec<PathBuf>)>> {
    // Determine paths to show in TUI: files from initial_scan_results and their ancestors.
    let mut paths_for_tui_display_set = HashSet::new();
    for (path, is_dir) in initial_scan_results {
        if !*is_dir {
            paths_for_tui_display_set.insert(path.clone());
            let mut current_ancestor = path.parent();
            while let Some(ancestor_path) = current_ancestor {
                if ancestor_path.starts_with(scan_root) || ancestor_path == scan_root {
                    paths_for_tui_display_set.insert(ancestor_path.to_path_buf());
                    if ancestor_path == scan_root {
                        break;
                    }
                    current_ancestor = ancestor_path.parent();
                } else {
                    break;
                }
            }
        }
    }
    if scan_root.is_dir()
        && (paths_for_tui_display_set
            .iter()
            .any(|p| p.starts_with(scan_root))
            || paths_for_tui_display_set.is_empty())
    {
        paths_for_tui_display_set.insert(scan_root.to_path_buf());
    }

    let path_to_is_dir_map: HashMap<PathBuf, bool> = initial_scan_results.iter().cloned().collect();
    let mut selectable_paths_for_tui: Vec<(PathBuf, bool)> = paths_for_tui_display_set
        .into_iter()
        .filter_map(|path| {
            path_to_is_dir_map
                .get(&path)
                .map(|is_dir| (path.clone(), *is_dir))
                .or_else(|| {
                    if path == scan_root && scan_root.is_dir() {
                        Some((path.clone(), true))
                    } else {
                        None
                    }
                })
        })
        .collect();

    selectable_paths_for_tui.sort_by(|(a, _), (b, _)| a.cmp(b));
    selectable_paths_for_tui.dedup_by(|(a, _), (b, _)| a == b);

    if selectable_paths_for_tui.is_empty() {
        return Ok(None); // No items to display in TUI.
    }

    // Prepare items for the TUI display.
    let display_labels = tree_builder::build_tree_labels(&selectable_paths_for_tui, scan_root);
    let mut prepared_tui_items =
        tui::prepare_selectable_items(&selectable_paths_for_tui, &display_labels, scan_root);

    // Apply --select globs for pre-selection in TUI.
    if !cli_args.select_globs.is_empty() {
        let preselect_glob_patterns: Vec<Pattern> = cli_args
            .select_globs
            .iter()
            .filter_map(|s| match Pattern::new(s) {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("⚠️ Warning: Invalid --select glob pattern '{}': {}", s, e);
                    std::process::exit(1);
                }
            })
            .collect();

        if !preselect_glob_patterns.is_empty() {
            let mut matched_item_indices = Vec::new();
            for (idx, item) in prepared_tui_items.iter().enumerate() {
                if !item.is_dir {
                    if let Ok(relative_path) = item.path.strip_prefix(scan_root) {
                        let path_to_match = if relative_path.as_os_str().is_empty() {
                            scan_root
                                .file_name()
                                .map(PathBuf::from)
                                .unwrap_or_else(|| relative_path.to_path_buf())
                        } else {
                            relative_path.to_path_buf()
                        };
                        if preselect_glob_patterns
                            .iter()
                            .any(|p| p.matches_path(&path_to_match))
                        {
                            matched_item_indices.push(idx);
                        }
                    }
                }
            }
            for &item_idx in &matched_item_indices {
                tui::apply_state_and_propagate_down_vec(
                    &mut prepared_tui_items,
                    item_idx,
                    tui::SelectionState::FullySelected,
                );
            }
            for &item_idx in &matched_item_indices {
                tui::update_all_parent_states_from_child_vec(&mut prepared_tui_items, item_idx);
            }
        }
    }

    // Run the TUI.
    match tui::run_tui_with_prepared_items(prepared_tui_items, scan_root)? {
        Some(final_tui_items_from_tui) => {
            // Process TUI selections.
            let mut files_to_yank_interactive: Vec<PathBuf> = final_tui_items_from_tui
                .iter()
                .filter(|item| !item.is_dir && item.state == tui::SelectionState::FullySelected)
                .map(|item| item.path.clone())
                .collect();
            files_to_yank_interactive.sort();
            files_to_yank_interactive.dedup();
            Ok(Some((final_tui_items_from_tui, files_to_yank_interactive)))
        }
        _ => Ok(None), // TUI cancelled by user.
    }
}

// Generates the final output string including the directory tree and file contents.
fn generate_output_string(
    final_tui_items_for_tree: &[tui::SelectableItem],
    files_to_yank: &[PathBuf],
    scan_root: &Path,
    all_paths_is_dir_map: &HashMap<PathBuf, bool>,
) -> Result<String> {
    // Determine nodes for the output tree display.
    let mut final_tree_node_paths_set = HashSet::new();
    if scan_root.exists() && scan_root.is_dir() {
        final_tree_node_paths_set.insert(scan_root.to_path_buf());
    }

    // Add selected/partially selected items and their ancestors from TUI/headless structured items.
    for item in final_tui_items_for_tree {
        if item.state == tui::SelectionState::FullySelected
            || item.state == tui::SelectionState::PartiallySelected
        {
            final_tree_node_paths_set.insert(item.path.clone());
            let mut current_ancestor = item.path.parent();
            while let Some(ancestor_path) = current_ancestor {
                if ancestor_path.starts_with(scan_root) || ancestor_path == scan_root {
                    final_tree_node_paths_set.insert(ancestor_path.to_path_buf());
                    if ancestor_path == scan_root {
                        break;
                    }
                    current_ancestor = ancestor_path.parent();
                } else {
                    break;
                }
            }
        }
    }
    // Ensure all actually yanked files and their ancestors are in the tree set.
    for file_path in files_to_yank {
        final_tree_node_paths_set.insert(file_path.clone());
        let mut current_ancestor = file_path.parent();
        while let Some(ancestor_path) = current_ancestor {
            if ancestor_path.starts_with(scan_root) || ancestor_path == scan_root {
                final_tree_node_paths_set.insert(ancestor_path.to_path_buf());
                if ancestor_path == scan_root {
                    break;
                }
                current_ancestor = ancestor_path.parent();
            } else {
                break;
            }
        }
    }

    let mut final_tree_nodes: Vec<(PathBuf, bool)> = final_tree_node_paths_set
        .into_iter()
        .map(|p| {
            (
                p.clone(),
                all_paths_is_dir_map
                    .get(&p)
                    .copied()
                    .unwrap_or_else(|| p.is_dir()),
            )
        })
        .collect();
    final_tree_nodes.sort_by(|(a, _), (b, _)| a.cmp(b));
    final_tree_nodes.dedup_by(|(a, _), (b, _)| a == b);

    // Build the tree part of the output.
    let output_tree_labels = tree_builder::build_tree_labels(&final_tree_nodes, scan_root);
    let mut output_string_parts: Vec<String> = Vec::new();
    for label in output_tree_labels {
        output_string_parts.push(label);
    }
    if !output_string_parts.is_empty() || !files_to_yank.is_empty() {
        output_string_parts.push("".to_string()); // Newline after tree.
    }

    // Append file contents.
    for file_path in files_to_yank {
        let relative_path = file_path.strip_prefix(scan_root).unwrap_or(file_path);
        match fs::read_to_string(file_path) {
            Ok(contents) => {
                output_string_parts.push(format!("---\nFile: {}\n---", relative_path.display()));
                output_string_parts.push("".to_string());
                output_string_parts.push(contents.trim_end().to_string());
                output_string_parts.push("".to_string());
            }
            Err(e) => {
                eprintln!(
                    "⚠️ Warning: Could not read file {}: {}",
                    file_path.display(),
                    e
                );
                output_string_parts.push(format!(
                    "---\nFile: {} (Error reading file: {})\n---",
                    relative_path.display(),
                    e
                ));
                output_string_parts.push("".to_string());
                output_string_parts.push("[Content not available]".to_string());
                output_string_parts.push("".to_string());
            }
        }
    }

    let mut final_output_string = output_string_parts.join("\n");
    if !final_output_string.is_empty() {
        // Ensure single trailing newline.
        final_output_string = final_output_string.trim_end_matches('\n').to_string();
        final_output_string.push('\n');
    }

    // Handle empty output case.
    if final_output_string.trim().is_empty() && files_to_yank.is_empty() {
        if scan_root.exists()
            && scan_root.is_dir()
            && final_tree_nodes.iter().any(|(p, _)| p == scan_root)
        {
            final_output_string = format!("./\n\n(No files selected or matched criteria)\n");
        } else {
            final_output_string = format!("(No files selected or matched criteria)\n");
        }
    }
    Ok(final_output_string)
}

// Performs the final action: printing for dry-run or copying to clipboard.
fn perform_final_action(
    output_string: &str,
    files_to_yank_count: usize,
    is_dry_run: bool,
    initial_scan_was_empty_and_not_default: bool,
) -> Result<()> {
    if is_dry_run {
        print!("{}", output_string);
        if files_to_yank_count == 0 {
            if !output_string.contains("(No files selected or matched criteria)")
                && !initial_scan_was_empty_and_not_default
            {
                println!("(Dry run: No files would have been copied based on selection/criteria)");
            }
        } else {
            let tokens = utils::approx_tokens(output_string);
            println!(
                "(Dry run: Would copy {} files (≈ {} tokens). Clipboard not affected.)",
                files_to_yank_count, tokens
            );
        }
    } else if files_to_yank_count == 0 {
        // This path should only be hit if something went wrong or an edge case led to no files
        // after initial checks passed.
        if !output_string.contains("(No files selected or matched criteria)") {
            println!("{}", output_string.trim_end());
        }
        println!("No files were ultimately selected to copy. Exiting.");
        std::process::exit(1); // Non-zero exit for actual copy operation with no files.
    } else {
        clipboard::copy_text_to_clipboard(output_string.to_string())?;
        let tokens = utils::approx_tokens(output_string);
        println!(
            "✅ Copied {} files (≈ {} tokens) to the clipboard.",
            files_to_yank_count, tokens
        );
    }
    Ok(())
}

// Main orchestrator for the repoyank application logic.
pub fn run_repoyank(cli_args: cli::Cli) -> Result<()> {
    // Step 1: Determine scan configuration (root directory and glob patterns).
    let (scan_root, glob_filter_patterns) = determine_scan_configuration(&cli_args)?;

    // Exit if all provided patterns were invalid (and patterns were actually provided, not just default).
    if glob_filter_patterns.is_empty()
        && !cli_args.patterns.is_empty()
        && !cli_args.patterns.iter().any(|p| p.as_str() == "**/*")
    {
        eprintln!("Error: All provided PATTERNs were invalid.");
        std::process::exit(1);
    }

    // Step 2: Gather initial candidate files and directories based on patterns and type filters.
    let initial_scan_results = gather_initial_candidates(
        &scan_root,
        &cli_args.type_filter,
        cli_args.include_ignored,
        &glob_filter_patterns,
    )?;

    // Flag to indicate if the initial scan yielded nothing with specific user-provided criteria.
    let initial_scan_was_empty_and_not_default_pattern = initial_scan_results.is_empty()
        && !glob_filter_patterns
            .iter()
            .any(|p| p.as_str() == "**/*" && cli_args.type_filter.is_empty());

    // If initial scan is empty with specific criteria, inform user and exit (unless dry-run).
    if initial_scan_was_empty_and_not_default_pattern {
        println!("No files matched the specified patterns and filters.");
        if !cli_args.dry_run {
            std::process::exit(1);
        }
        // For dry run, continue to generate the "(No files...)" output.
    }

    // Step 3: Dispatch to headless (--all) mode or interactive TUI mode.
    let (final_tui_items_for_tree, mut files_to_yank) = if cli_args.all {
        // Headless mode.
        let (items, yanks) = run_headless_mode(&initial_scan_results, &scan_root)?;
        if yanks.is_empty() && !cli_args.dry_run && !initial_scan_was_empty_and_not_default_pattern
        {
            println!("No files matched the specified criteria for yanking in --all mode.");
            std::process::exit(1);
        }
        (items, yanks)
    } else {
        // Interactive TUI mode.
        match run_interactive_mode(&initial_scan_results, &cli_args, &scan_root)? {
            Some(result) => result, // TUI successful, result contains (items_for_tree, yanks)
            None => {
                // TUI was cancelled or had no items to display.
                if initial_scan_was_empty_and_not_default_pattern && cli_args.dry_run {
                    // Proceed with empty results for dry run to show "(No files...)" output.
                    (Vec::new(), Vec::new())
                } else if initial_scan_results.is_empty()
                    && !cli_args.dry_run
                    && !initial_scan_was_empty_and_not_default_pattern
                {
                    // TUI had no items because initial scan was empty (and not default pattern).
                    println!("No matching files or directories found to select from in TUI.");
                    std::process::exit(1);
                } else {
                    // TUI cancelled by user, or TUI had no items for other reasons.
                    println!("Selection cancelled or no items to display. Exiting.");
                    return Ok(()); // User cancellation is a graceful exit.
                }
            }
        }
    };

    // Ensure files_to_yank is sorted and deduped for consistent output.
    files_to_yank.sort();
    files_to_yank.dedup();

    // If, after mode processing, no files are selected for yanking (and not dry-run, and initial scan wasn't already empty and handled).
    if files_to_yank.is_empty()
        && !cli_args.dry_run
        && !initial_scan_was_empty_and_not_default_pattern
    {
        println!("No files selected or matched criteria to copy.");
        std::process::exit(1);
    }

    // Step 4: Prepare data for final output string generation.
    // Get a comprehensive map of all paths under scan_root for accurate is_dir info for the tree.
    let all_paths_is_dir_map: HashMap<PathBuf, bool> =
        file_scanner::scan_files(&scan_root, &[], true)?
            .into_iter()
            .collect();

    // Generate the final output string (tree + file contents).
    let output_string = generate_output_string(
        &final_tui_items_for_tree,
        &files_to_yank,
        &scan_root,
        &all_paths_is_dir_map,
    )?;

    // Step 5: Perform the final action (dry-run print or copy to clipboard).
    perform_final_action(
        &output_string,
        files_to_yank.len(),
        cli_args.dry_run,
        initial_scan_was_empty_and_not_default_pattern, // Pass this to refine "no files" messages.
    )?;

    Ok(())
}
