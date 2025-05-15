use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionState {
    NotSelected,
    PartiallySelected,
    FullySelected,
}

#[derive(Debug, Clone)]
pub struct SelectableItem {
    pub path: PathBuf,
    pub display_text: String,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub state: SelectionState,
    pub children_indices: Vec<usize>,
    pub parent_index: Option<usize>,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) enum AppMode {
    // pub(super) for use within tui module
    Normal,
    Filtering,
}
