use super::app_state::{AppMode, SelectableItem, SelectionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// --- Propagation Helpers (public to the crate via tui/mod.rs re-export) ---
pub fn apply_state_and_propagate_down_vec(
    items: &mut [SelectableItem],
    item_idx: usize,
    new_state: SelectionState,
) {
    if item_idx >= items.len() {
        return;
    }
    let actual_new_state =
        if !items[item_idx].is_dir && new_state == SelectionState::PartiallySelected {
            SelectionState::FullySelected
        } else {
            new_state
        };
    items[item_idx].state = actual_new_state;
    if items[item_idx].is_dir && actual_new_state != SelectionState::PartiallySelected {
        let children_indices = items[item_idx].children_indices.clone();
        for child_idx in children_indices {
            apply_state_and_propagate_down_vec(items, child_idx, actual_new_state);
        }
    }
}

fn recalculate_parent_state_vec(items: &mut [SelectableItem], parent_idx: usize) {
    if parent_idx >= items.len() || !items[parent_idx].is_dir {
        return;
    }
    let children_indices = items[parent_idx].children_indices.clone();
    if children_indices.is_empty() {
        return;
    } // No children, no change based on them

    let mut num_fully_selected_children = 0;
    let mut num_partially_selected_children = 0;
    for &child_idx in &children_indices {
        if child_idx >= items.len() {
            continue;
        }
        match items[child_idx].state {
            SelectionState::FullySelected => num_fully_selected_children += 1,
            SelectionState::PartiallySelected => num_partially_selected_children += 1,
            SelectionState::NotSelected => {}
        }
    }
    let total_children = children_indices.len();
    items[parent_idx].state = if num_fully_selected_children == total_children && total_children > 0
    {
        SelectionState::FullySelected // Only fully selected if all children are fully selected AND there are children
    } else if num_fully_selected_children > 0 || num_partially_selected_children > 0 {
        SelectionState::PartiallySelected
    } else {
        SelectionState::NotSelected
    };
}

pub fn update_all_parent_states_from_child_vec(items: &mut [SelectableItem], child_idx: usize) {
    if child_idx >= items.len() {
        return;
    }
    let mut current_parent_idx_opt = items[child_idx].parent_index;
    while let Some(parent_idx) = current_parent_idx_opt {
        recalculate_parent_state_vec(items, parent_idx);
        if parent_idx >= items.len() {
            break;
        } // Should not happen
        current_parent_idx_opt = items[parent_idx].parent_index;
    }
}

// --- TuiApp struct and impl ---
pub struct TuiApp {
    pub(super) items: Vec<SelectableItem>,
    pub(super) current_selection_idx: usize,
    pub(super) scroll_offset: usize,
    pub(super) quit: bool,
    pub(super) confirmed: bool,
    pub(super) mode: AppMode,
    pub(super) filter_input: String,
    pub(super) filter_cursor_pos: usize,
    pub(super) list_viewport_height: usize,
}

impl TuiApp {
    pub fn new(items: Vec<SelectableItem>) -> Self {
        TuiApp {
            items,
            current_selection_idx: 0,
            scroll_offset: 0,
            quit: false,
            confirmed: false,
            mode: AppMode::Normal,
            filter_input: String::new(),
            filter_cursor_pos: 0,
            list_viewport_height: 0, // Will be updated by ui_renderer
        }
    }

    pub(super) fn select_next_visible_item(&mut self) {
        self.move_selection_in_visible_list(1);
    }

    pub(super) fn select_previous_visible_item(&mut self) {
        self.move_selection_in_visible_list(-1);
    }

    pub(super) fn move_selection_in_visible_list(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let visible_indices = self.get_visible_item_indices();
        if visible_indices.is_empty() {
            return;
        }

        let current_item_position_in_visible_list = visible_indices
            .iter()
            .position(|&idx| idx == self.current_selection_idx);

        let new_idx_in_visible_list = match current_item_position_in_visible_list {
            Some(pos) => (pos as i32 + delta).rem_euclid(visible_indices.len() as i32) as usize,
            None => {
                if delta > 0 || visible_indices.is_empty() {
                    0
                } else {
                    visible_indices.len() - 1
                }
            }
        };

        if !visible_indices.is_empty() {
            self.current_selection_idx = visible_indices[new_idx_in_visible_list];
        } else if !self.items.is_empty() {
            // Should not be reachable if visible_indices is empty but items is not
            self.current_selection_idx = 0;
        }
    }

    pub(super) fn toggle_current_item_selection(&mut self) {
        if self.items.is_empty() || self.current_selection_idx >= self.items.len() {
            return;
        }
        let item_idx = self.current_selection_idx;
        let current_item_state = self.items[item_idx].state;
        let new_state_for_item = match current_item_state {
            SelectionState::NotSelected | SelectionState::PartiallySelected => {
                SelectionState::FullySelected
            }
            SelectionState::FullySelected => SelectionState::NotSelected,
        };
        apply_state_and_propagate_down_vec(&mut self.items, item_idx, new_state_for_item);
        update_all_parent_states_from_child_vec(&mut self.items, item_idx);
    }

    pub(super) fn select_all_visible_items(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        for &item_idx in &visible_indices {
            if !self.items[item_idx].is_dir {
                apply_state_and_propagate_down_vec(
                    &mut self.items,
                    item_idx,
                    SelectionState::FullySelected,
                );
                update_all_parent_states_from_child_vec(&mut self.items, item_idx);
            }
        }
    }

    pub(super) fn deselect_all_visible_items(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        for &item_idx in &visible_indices {
            apply_state_and_propagate_down_vec(
                &mut self.items,
                item_idx,
                SelectionState::NotSelected,
            );
            update_all_parent_states_from_child_vec(&mut self.items, item_idx);
        }
    }

    pub(super) fn expand_all_directories(&mut self) {
        for item in self.items.iter_mut() {
            if item.is_dir {
                item.is_expanded = true;
            }
        }
        self.ensure_selection_is_visible(); // This one, not viewport specific
    }

    pub(super) fn collapse_all_directories(&mut self) {
        let root_path_of_tui = self.items.first().map(|item| item.path.clone());
        for item in self.items.iter_mut() {
            if item.is_dir {
                if item.parent_index.is_some() || Some(&item.path) != root_path_of_tui.as_ref() {
                    item.is_expanded = false;
                } else {
                    item.is_expanded = true;
                }
            }
        }
        self.ensure_selection_is_visible(); // This one, not viewport specific
    }

    pub(super) fn get_visible_item_indices(&self) -> Vec<usize> {
        let mut visible_indices = Vec::new();
        let filter_active = !self.filter_input.is_empty();
        let lower_filter = self.filter_input.to_lowercase();

        for i in 0..self.items.len() {
            if self.is_item_visible_recursive(i) {
                if filter_active {
                    if self.item_matches_filter_or_has_matching_descendant(i, &lower_filter) {
                        visible_indices.push(i);
                    }
                } else {
                    visible_indices.push(i);
                }
            }
        }
        visible_indices
    }

    pub(super) fn is_item_visible_recursive(&self, item_idx: usize) -> bool {
        if item_idx >= self.items.len() {
            return false;
        }
        let item = &self.items[item_idx];
        match item.parent_index {
            None => true,
            Some(parent_idx) => {
                if parent_idx >= self.items.len() {
                    return false;
                }
                self.items[parent_idx].is_expanded && self.is_item_visible_recursive(parent_idx)
            }
        }
    }

    pub(super) fn item_matches_filter_or_has_matching_descendant(
        &self,
        item_idx: usize,
        lower_filter: &str,
    ) -> bool {
        if item_idx >= self.items.len() {
            return false;
        }
        let item = &self.items[item_idx];
        if item.display_text.to_lowercase().contains(lower_filter) {
            return true;
        }
        if item.is_dir {
            for &child_idx in &item.children_indices {
                if self.item_matches_filter_or_has_matching_descendant(child_idx, lower_filter) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn ensure_selection_is_valid_after_filter(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        if visible_indices.is_empty() {
            return;
        }
        if !visible_indices.contains(&self.current_selection_idx) {
            self.current_selection_idx = *visible_indices.first().unwrap_or(&0);
        }
        // After selection index is valid, then ensure viewport is correct.
        // This might be better called from the main loop or ui_frame.
        self.ensure_selection_is_visible_in_viewport();
    }

    pub(super) fn ensure_selection_is_visible_in_viewport(&mut self) {
        if self.items.is_empty() || self.list_viewport_height == 0 {
            return;
        }
        let visible_indices = self.get_visible_item_indices();
        if visible_indices.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        let list_height = self.list_viewport_height;
        let current_item_position_in_visible_list = visible_indices
            .iter()
            .position(|&idx| idx == self.current_selection_idx);

        if let Some(pos) = current_item_position_in_visible_list {
            if pos < self.scroll_offset {
                self.scroll_offset = pos;
            } else if pos >= self.scroll_offset + list_height {
                self.scroll_offset = pos.saturating_sub(list_height - 1);
            }
        } else if !visible_indices.is_empty() {
            // Selection valid but not in current viewport logic path
            self.current_selection_idx = *visible_indices.first().unwrap_or(&0); // Should be ensured by ensure_selection_is_valid_after_filter
            self.scroll_offset = 0;
        }

        let num_visible_items = visible_indices.len();
        if num_visible_items <= list_height {
            self.scroll_offset = 0;
        } else {
            self.scroll_offset = self.scroll_offset.min(num_visible_items - list_height);
        }
        self.scroll_offset = self.scroll_offset.max(0);
    }

    pub(super) fn toggle_expansion_and_adjust_selection(&mut self) {
        if self.items.is_empty() || self.current_selection_idx >= self.items.len() {
            return;
        }
        let item_idx = self.current_selection_idx;
        if self.items[item_idx].is_dir {
            self.items[item_idx].is_expanded = !self.items[item_idx].is_expanded;
            self.ensure_selection_is_visible(); // Hierarchical visibility check
        }
    }

    // This is the original ensure_selection_is_visible, focused on hierarchical adjustment
    pub(super) fn ensure_selection_is_visible(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.is_item_visible_recursive(self.current_selection_idx) {
            // Use hierarchical check
            self.ensure_selection_is_visible_in_viewport(); // Then adjust viewport
            return;
        }
        let mut candidate_idx = self.current_selection_idx;
        while let Some(parent_idx) = self
            .items
            .get(candidate_idx)
            .and_then(|item| item.parent_index)
        {
            candidate_idx = parent_idx;
            if self.is_item_visible_recursive(candidate_idx) {
                self.current_selection_idx = candidate_idx;
                self.ensure_selection_is_visible_in_viewport();
                return;
            }
        }
        // If loop finishes, candidate_idx is likely a root item. Check its visibility.
        if self.is_item_visible_recursive(candidate_idx) {
            self.current_selection_idx = candidate_idx;
            self.ensure_selection_is_visible_in_viewport();
            return;
        }
        // Fallback: move to the first truly visible item
        let visible_indices = self.get_visible_item_indices(); // This is filter-aware
        if let Some(&first_visible_idx) = visible_indices.first() {
            self.current_selection_idx = first_visible_idx;
        } else if !self.items.is_empty() {
            // No items visible at all
            self.current_selection_idx = 0; // Default to first item, even if not visible
        }
        self.ensure_selection_is_visible_in_viewport();
    }

    // --- Event handling sub-methods ---
    pub(super) fn handle_normal_mode_input(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('/') => {
                self.mode = AppMode::Filtering;
            }
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('y') => {
                self.confirmed = true;
                self.quit = true;
            }
            KeyCode::Down | KeyCode::Char('j') => self.select_next_visible_item(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_visible_item(),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_current_item_selection(),
            KeyCode::Char('o') | KeyCode::Tab => self.toggle_expansion_and_adjust_selection(),
            KeyCode::Char('*') => self.expand_all_directories(),
            KeyCode::Char('-') => self.collapse_all_directories(),
            KeyCode::Char('a') => {
                if key_event.modifiers.is_empty() || key_event.modifiers == KeyModifiers::CONTROL {
                    self.select_all_visible_items();
                }
            }
            KeyCode::Char('A') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.select_all_visible_items();
            }
            KeyCode::Char('d') => {
                if key_event.modifiers.is_empty() {
                    self.deselect_all_visible_items();
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_filtering_mode_input(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Enter => {
                self.mode = AppMode::Normal;
                self.ensure_selection_is_valid_after_filter();
            }
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
                self.filter_input.clear();
                self.filter_cursor_pos = 0;
                self.ensure_selection_is_valid_after_filter();
            }
            KeyCode::Char(c) => {
                self.filter_input.insert(self.filter_cursor_pos, c);
                self.filter_cursor_pos += 1;
                self.ensure_selection_is_valid_after_filter();
            }
            KeyCode::Backspace => {
                if self.filter_cursor_pos > 0 && !self.filter_input.is_empty() {
                    self.filter_cursor_pos -= 1;
                    self.filter_input.remove(self.filter_cursor_pos);
                    self.ensure_selection_is_valid_after_filter();
                }
            }
            KeyCode::Left => {
                if self.filter_cursor_pos > 0 {
                    self.filter_cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.filter_cursor_pos < self.filter_input.len() {
                    self.filter_cursor_pos += 1;
                }
            }
            _ => {}
        }
    }
}

// --- prepare_selectable_items (public to the crate via tui/mod.rs re-export) ---
pub fn prepare_selectable_items(
    initial_items_paths_is_dir: &[(PathBuf, bool)],
    display_labels: &[String],
    root_path: &Path,
) -> Vec<SelectableItem> {
    let mut selectable_items = Vec::new();
    let mut path_to_idx_map: HashMap<PathBuf, usize> = HashMap::new();
    for (i, ((path, is_dir), label)) in initial_items_paths_is_dir
        .iter()
        .zip(display_labels.iter())
        .enumerate()
    {
        path_to_idx_map.insert(path.clone(), i);
        selectable_items.push(SelectableItem {
            path: path.clone(),
            display_text: label.clone(),
            is_dir: *is_dir,
            is_expanded: *is_dir, // Default to expanded
            state: SelectionState::NotSelected,
            children_indices: Vec::new(),
            parent_index: None,
        });
    }
    for i in 0..selectable_items.len() {
        let path = selectable_items[i].path.clone();
        // Only set parent if not the root_path itself (or an item representing root)
        if path != *root_path
            && path.parent().is_some()
            && (path.parent().unwrap() == root_path
                || path.parent().unwrap().starts_with(root_path))
        {
            if let Some(parent_pbuf) = path.parent() {
                if let Some(parent_idx) = path_to_idx_map.get(parent_pbuf) {
                    selectable_items[i].parent_index = Some(*parent_idx);
                    if selectable_items[*parent_idx].is_dir {
                        // Ensure parent is actually a directory
                        selectable_items[*parent_idx].children_indices.push(i);
                    }
                }
            }
        }
    }
    selectable_items
}
