use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Build pretty tree-style labels in **O(n)**.
///
/// * `paths` **must** be lexicographically sorted.
/// * Each element in `paths` is `(path, is_dir)`.
pub fn build_tree_labels(paths: &[(PathBuf, bool)], root_path: &Path) -> Vec<String> {
    let n = paths.len();
    let mut labels = Vec::with_capacity(n);
    // is_last_for_ancestor_at_depth[d] is true if the ancestor at depth 'd' is the last child of *its* parent.
    // This is used to decide whether to draw "│  " or "   ".
    // More directly, `is_last_vec[i]` tells if path `i` is the last among its siblings.
    let mut is_last_vec = vec![false; n]; // is_last_vec[i] is true if paths[i] is the last child of its parent.
    let mut last_child_map = HashMap::<PathBuf, usize>::new(); // parent_path -> index_of_last_child_in_paths

    // PASS #1 – record each directory’s last immediate child index
    for (idx, (path, _)) in paths.iter().enumerate() {
        let rel = path.strip_prefix(root_path).unwrap_or(path);
        let parent = rel.parent().unwrap_or_else(|| Path::new(""));
        last_child_map.insert(parent.to_path_buf(), idx);
    }

    // PASS #2 – scan once, using a stack to track ancestor indices
    let mut ancestor_stack: Vec<usize> = Vec::new(); // Stores indices from `paths` for current ancestors
    for (idx, (path, is_dir)) in paths.iter().enumerate() {
        let rel = path.strip_prefix(root_path).unwrap_or(path);
        // Depth: number of components in relative path. Root "." is depth 0. "./foo" is depth 1.
        let depth = if rel == Path::new(".") || rel.as_os_str().is_empty() {
            0
        } else {
            rel.components().filter(|c| *c != Component::CurDir).count()
        };

        // Adjust stack to current depth: pop ancestors that are no longer relevant
        // Stack length should be == current depth before processing prefix
        while ancestor_stack.len() > depth {
            ancestor_stack.pop();
        }

        let parent_rel_path = rel.parent().unwrap_or_else(|| Path::new(""));
        let is_last_child = last_child_map.get(&parent_rel_path.to_path_buf()) == Some(&idx);
        is_last_vec[idx] = is_last_child;

        let mut prefix = String::new();
        if depth > 0 {
            // Only add prefix for items not at the root level
            // For ancestors *above* the current item's parent (i.e., stack elements from index 1 up to depth-1)
            // ancestor_stack[0] is the root, ancestor_stack[1] is child of root, etc.
            // We need to look at ancestors up to current_depth - 1.
            // ancestor_stack[d] is the index of the ancestor at depth d.
            if depth > 1 {
                // If current item has grandparents or older ancestors
                for &ancestor_idx_on_stack in &ancestor_stack[1..depth] {
                    // Skip root (stack[0]), go up to parent's level
                    prefix.push_str(if is_last_vec[ancestor_idx_on_stack] {
                        "   "
                    } else {
                        "│  "
                    });
                }
            }
            prefix.push_str(if is_last_child { "└─ " } else { "├─ " });
        }

        let name = rel
            .file_name()
            .unwrap_or_else(|| {
                std::ffi::OsStr::new(if rel.as_os_str().is_empty() || rel == Path::new(".") {
                    "."
                } else {
                    "." /* should not happen if path is not root */
                })
            })
            .to_string_lossy();

        let label = if path == root_path || (rel.as_os_str().is_empty() || rel == Path::new(".")) {
            "./".to_string()
        } else if *is_dir {
            format!("{}{}/", prefix, name)
        } else {
            format!("{}{}", prefix, name)
        };
        labels.push(label);

        // If current path's depth is equal to stack length, it means we are descending or staying at same level.
        // If current path's depth is less than stack length, it means we moved up, stack already popped.
        if depth == ancestor_stack.len() {
            ancestor_stack.push(idx);
        } else if depth < ancestor_stack.len() {
            // This case should be handled by the `while ancestor_stack.len() > depth` loop.
            // If we are here, it means we are at a new sibling or a shallower path.
            // The stack should already be correct. We update the entry for the current depth.
            ancestor_stack[depth] = idx;
        } else {
            // depth > ancestor_stack.len() -- should not happen if logic is correct
            // This would mean a jump in depth, e.g. from depth 1 to depth 3 without a depth 2.
            // This implies paths are not correctly ordered or structure is broken.
            // For safety, push, but this might indicate an issue.
            while ancestor_stack.len() <= depth {
                ancestor_stack.push(0); // Placeholder, will be overwritten
            }
            ancestor_stack[depth] = idx;
        }
    }
    labels
}
