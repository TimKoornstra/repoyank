use anyhow::Result;
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn scan_files(
    root: &Path,
    types_filter: &[String],
    include_ignored: bool,
) -> Result<Vec<(PathBuf, bool)>> {
    let mut collected_paths: Vec<(PathBuf, bool)> = Vec::new();
    let mut walker = WalkBuilder::new(root);

    if include_ignored {
        walker.git_ignore(false).ignore(false);
    }
    // Ensure the root directory itself is always included if it exists,
    // especially if it's empty or only contains filtered-out files.
    // It's important for build_tree_labels to have the root.
    if root.exists() && root.is_dir() {
        collected_paths.push((root.to_path_buf(), true));
    }

    for result in walker.build() {
        let dirent = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠️  Warning during scan: {}", e);
                continue;
            }
        };

        let path = dirent.into_path();

        // Skip the root path itself if already added, to avoid duplicates from walker
        if path == root {
            continue;
        }

        let is_dir = path.is_dir();

        if !types_filter.is_empty() && !is_dir {
            // Apply type filter only to files
            let keep = types_filter
                .iter()
                .any(|ext_filter_str| path.extension() == Some(OsStr::new(ext_filter_str)));
            if !keep {
                continue;
            }
        }
        collected_paths.push((path, is_dir));
    }

    collected_paths.sort_by(|(a, _), (b, _)| a.cmp(b));
    collected_paths.dedup_by(|(a, _), (b, _)| a == b); // Deduplicate, root might be added twice

    Ok(collected_paths)
}
