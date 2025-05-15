use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use std::{
    collections::HashMap,
    io::{self, Stdout},
    path::{Path, PathBuf},
    time::Duration,
};

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
    // For propagating selections:
    pub children_indices: Vec<usize>,
    pub parent_index: Option<usize>,
}

#[derive(PartialEq, Eq)]
enum AppMode {
    Normal,
    Filtering,
}

// Helper function to apply state and propagate down for pre-selection
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
            // Files cannot be PartiallySelected from a direct "select all children" type operation
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

// Helper function to recalculate parent state for pre-selection
fn recalculate_parent_state_vec(items: &mut [SelectableItem], parent_idx: usize) {
    if parent_idx >= items.len() || !items[parent_idx].is_dir {
        return;
    }

    let children_indices = items[parent_idx].children_indices.clone();
    if children_indices.is_empty() {
        // If a dir has no selectable children its state does not change based on children
        return;
    }

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
    let new_parent_state = if num_fully_selected_children == total_children {
        SelectionState::FullySelected
    } else if num_fully_selected_children == 0 && num_partially_selected_children == 0 {
        SelectionState::NotSelected
    } else {
        SelectionState::PartiallySelected
    };

    items[parent_idx].state = new_parent_state;
}

// Helper function to update all parent states from a child for pre-selection
pub fn update_all_parent_states_from_child_vec(items: &mut [SelectableItem], child_idx: usize) {
    if child_idx >= items.len() {
        return;
    }
    let mut current_parent_idx_opt = items[child_idx].parent_index;
    while let Some(parent_idx) = current_parent_idx_opt {
        recalculate_parent_state_vec(items, parent_idx);
        if parent_idx >= items.len() {
            break;
        }
        current_parent_idx_opt = items[parent_idx].parent_index;
    }
}

pub struct TuiApp {
    items: Vec<SelectableItem>,
    current_selection_idx: usize,
    scroll_offset: usize,
    quit: bool,
    confirmed: bool,
    mode: AppMode,
    filter_input: String,
    filter_cursor_pos: usize,
}

impl TuiApp {
    fn new(items: Vec<SelectableItem>) -> Self {
        TuiApp {
            items,
            current_selection_idx: 0,
            scroll_offset: 0,
            quit: false,
            confirmed: false,
            mode: AppMode::Normal,
            filter_input: String::new(),
            filter_cursor_pos: 0,
        }
    }

    fn select_next_visible_item(&mut self) {
        self.move_selection_in_visible_list(1);
    }

    fn select_previous_visible_item(&mut self) {
        self.move_selection_in_visible_list(-1);
    }

    fn move_selection_in_visible_list(&mut self, delta: i32) {
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
            self.current_selection_idx = 0;
        }
    }

    fn toggle_current_item_selection(&mut self) {
        if self.items.is_empty() {
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

    fn select_all_visible_items(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        for &item_idx in &visible_indices {
            if !self.items[item_idx].is_dir {
                // Only select files
                // Apply FullySelected state directly to the file
                apply_state_and_propagate_down_vec(
                    &mut self.items,
                    item_idx,
                    SelectionState::FullySelected,
                );
                // Then update its parents
                update_all_parent_states_from_child_vec(&mut self.items, item_idx);
            }
        }
    }

    fn deselect_all_visible_items(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        for &item_idx in &visible_indices {
            // Apply NotSelected state and propagate down (will unselect children if it's a dir)
            apply_state_and_propagate_down_vec(
                &mut self.items,
                item_idx,
                SelectionState::NotSelected,
            );
            // Then update its parents
            update_all_parent_states_from_child_vec(&mut self.items, item_idx);
        }
    }

    fn expand_all_directories(&mut self) {
        for item in self.items.iter_mut() {
            if item.is_dir {
                item.is_expanded = true;
            }
        }
        self.ensure_selection_is_visible(); // Selection might have become hidden
    }

    fn collapse_all_directories(&mut self) {
        let root_path_of_tui = self.items.first().map(|item| item.path.clone());

        for item in self.items.iter_mut() {
            if item.is_dir {
                // Don't collapse the root item if it's the only thing visible or if it's explicitly the root
                // A simple heuristic: don't collapse if it has no parent (i.e., it's a root-level item in the TUI list)
                if item.parent_index.is_some() || Some(&item.path) != root_path_of_tui.as_ref() {
                    item.is_expanded = false;
                } else {
                    item.is_expanded = true; // Ensure root-level items remain expanded
                }
            }
        }
        self.ensure_selection_is_visible();
    }

    fn is_item_visible(&self, item_idx: usize) -> bool {
        let item = &self.items[item_idx];
        match item.parent_index {
            None => true, // Root items are always visible
            Some(parent_idx) => {
                // Visible if parent is visible AND parent is expanded
                self.items[parent_idx].is_expanded && self.is_item_visible(parent_idx)
            }
        }
    }

    fn get_visible_item_indices(&self) -> Vec<usize> {
        let mut visible_indices = Vec::new();
        let filter_active = !self.filter_input.is_empty();
        let lower_filter = self.filter_input.to_lowercase();

        for i in 0..self.items.len() {
            if self.is_item_visible_recursive(i) {
                // Check recursive visibility first
                if filter_active {
                    // If filtering, item must match OR be an ancestor of a matched item
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

    fn is_item_visible_recursive(&self, item_idx: usize) -> bool {
        if item_idx >= self.items.len() {
            return false;
        }
        let item = &self.items[item_idx];
        match item.parent_index {
            None => true, // Root items are always visible (in terms of hierarchy)
            Some(parent_idx) => {
                if parent_idx >= self.items.len() {
                    return false;
                } // Should not happen
                self.items[parent_idx].is_expanded && self.is_item_visible_recursive(parent_idx)
            }
        }
    }

    // Helper to check if an item itself matches or any of its descendants match
    fn item_matches_filter_or_has_matching_descendant(
        &self,
        item_idx: usize,
        lower_filter: &str,
    ) -> bool {
        if item_idx >= self.items.len() {
            return false;
        }
        let item = &self.items[item_idx];

        // Check if current item matches
        // Using display_text which includes the tree prefix. Alternatively, match on path.
        if item.display_text.to_lowercase().contains(lower_filter) {
            return true;
        }

        // If it's a directory, check its children recursively
        if item.is_dir {
            for &child_idx in &item.children_indices {
                if self.item_matches_filter_or_has_matching_descendant(child_idx, lower_filter) {
                    return true; // A descendant matches
                }
            }
        }
        false // No match for this item or its descendants
    }

    fn ensure_selection_is_valid_after_filter(&mut self) {
        let visible_indices = self.get_visible_item_indices();
        if visible_indices.is_empty() {
            // No items match the filter. The list will just appear empty.
            return;
        }

        if !visible_indices.contains(&self.current_selection_idx) {
            // Current selection is not in the filtered list, move to the first visible.
            self.current_selection_idx = *visible_indices.first().unwrap_or(&0);
        }
        self.ensure_selection_is_visible_in_viewport();
    }

    fn ensure_selection_is_visible_in_viewport(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let visible_item_indices = self.get_visible_item_indices();
        if visible_item_indices.is_empty() {
            return;
        }

        // If current_selection_idx is NOT EVEN IN THE HIERARCHICALLY EXPANDED AND FILTERED LIST,
        // we must reset it. This is partly handled by ensure_selection_is_valid_after_filter.
        // This function then focuses on scrolling.

        // The rest of this function is about scrolling the viewport
        // based on current_selection_idx relative to the visible_item_indices.
        // The logic here for adjusting scroll_offset remains largely the same.
        // The key is that `visible_item_indices` is now filter-aware.

        let list_height = 20; // Placeholder, get from frame in ui_frame
        // This value needs to be passed or calculated based on frame size.
        // For now, using a constant for internal logic.

        let current_selected_item_in_visible_list_idx_opt = visible_item_indices
            .iter()
            .position(|&idx| idx == self.current_selection_idx);

        if let Some(selected_idx_in_visible_list) = current_selected_item_in_visible_list_idx_opt {
            if selected_idx_in_visible_list < self.scroll_offset {
                self.scroll_offset = selected_idx_in_visible_list;
            } else if selected_idx_in_visible_list >= self.scroll_offset + list_height {
                self.scroll_offset = selected_idx_in_visible_list - list_height + 1;
            }
        } else if !visible_item_indices.is_empty() {
            // Selection is not in visible list (e.g. due to filter change)
            self.current_selection_idx = *visible_item_indices.first().unwrap_or(&0);
            self.scroll_offset = 0;
        }

        let num_visible_items = visible_item_indices.len();
        if num_visible_items == 0 {
            self.scroll_offset = 0;
        } else if num_visible_items < list_height {
            self.scroll_offset = 0;
        } else if self.scroll_offset > num_visible_items.saturating_sub(list_height) {
            self.scroll_offset = num_visible_items.saturating_sub(list_height);
        }
        self.scroll_offset = self.scroll_offset.max(0);
        if num_visible_items > 0 {
            // Avoid panic on num_visible_items.saturating_sub(1) if empty
            self.scroll_offset = self.scroll_offset.min(num_visible_items.saturating_sub(1));
        }
    }

    fn toggle_expansion_and_adjust_selection(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let item_idx = self.current_selection_idx;

        if self.items[item_idx].is_dir {
            self.items[item_idx].is_expanded = !self.items[item_idx].is_expanded;
            self.ensure_selection_is_visible();
        }
    }

    fn ensure_selection_is_visible(&mut self) {
        if self.items.is_empty() {
            return;
        }

        if self.is_item_visible(self.current_selection_idx) {
            return;
        }

        let mut candidate_idx = self.current_selection_idx;
        while let Some(parent_idx) = self.items[candidate_idx].parent_index {
            candidate_idx = parent_idx;
            if self.is_item_visible(candidate_idx) {
                self.current_selection_idx = candidate_idx;
                return;
            }
        }

        if self.is_item_visible(candidate_idx) {
            self.current_selection_idx = candidate_idx;
            return;
        }

        let visible_indices = self.get_visible_item_indices();
        if let Some(&first_visible_idx) = visible_indices.first() {
            self.current_selection_idx = first_visible_idx;
        } else if !self.items.is_empty() {
            self.current_selection_idx = 0;
        }
    }
}

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
            is_expanded: *is_dir,
            state: SelectionState::NotSelected,
            children_indices: Vec::new(),
            parent_index: None,
        });
    }

    for i in 0..selectable_items.len() {
        let path = selectable_items[i].path.clone();
        if path != *root_path {
            if let Some(parent_pbuf) = path.parent() {
                if parent_pbuf.starts_with(root_path) || parent_pbuf == root_path {
                    if let Some(parent_idx) = path_to_idx_map.get(parent_pbuf) {
                        selectable_items[i].parent_index = Some(*parent_idx);
                        if selectable_items[*parent_idx].is_dir {
                            selectable_items[*parent_idx].children_indices.push(i);
                        }
                    }
                }
            }
        }
    }
    selectable_items
}

pub fn run_tui_with_prepared_items(
    prepared_items: Vec<SelectableItem>,
    #[allow(unused_variables)] root_path: &Path,
) -> Result<Option<Vec<SelectableItem>>> {
    if prepared_items.is_empty() {
        return Ok(None);
    }
    let mut app = TuiApp::new(prepared_items);

    let mut terminal = init_terminal()?;
    app.ensure_selection_is_visible();

    while !app.quit {
        terminal.draw(|frame| ui_frame(frame, &mut app))?;
        handle_events(&mut app)?;
    }

    restore_terminal(terminal)?;

    if app.confirmed {
        Ok(Some(app.items))
    } else {
        Ok(None)
    }
}
fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(Into::into)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor().map_err(Into::into)
}

fn handle_events(app: &mut TuiApp) -> Result<()> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                if app.mode == AppMode::Filtering {
                    match key_event.code {
                        KeyCode::Enter => {
                            app.mode = AppMode::Normal;
                            // Filter is applied, ensure selection is valid
                            app.ensure_selection_is_valid_after_filter();
                        }
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                            app.filter_input.clear();
                            app.filter_cursor_pos = 0;
                            app.ensure_selection_is_valid_after_filter();
                        }
                        KeyCode::Char(c) => {
                            // Insert char at cursor position
                            app.filter_input.insert(app.filter_cursor_pos, c);
                            app.filter_cursor_pos += 1;
                            app.ensure_selection_is_valid_after_filter();
                        }
                        KeyCode::Backspace => {
                            if app.filter_cursor_pos > 0 && !app.filter_input.is_empty() {
                                app.filter_cursor_pos -= 1;
                                app.filter_input.remove(app.filter_cursor_pos);
                                app.ensure_selection_is_valid_after_filter();
                            }
                        }
                        KeyCode::Left => {
                            if app.filter_cursor_pos > 0 {
                                app.filter_cursor_pos -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if app.filter_cursor_pos < app.filter_input.len() {
                                app.filter_cursor_pos += 1;
                            }
                        }
                        _ => {}
                    }
                    return Ok(());
                }

                // Handle primary character key presses (no complex modifiers expected for these actions)
                match key_event.code {
                    KeyCode::Char('/') => {
                        // Enter Filtering mode
                        app.mode = AppMode::Filtering;
                    }
                    KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
                    KeyCode::Char('y') => {
                        app.confirmed = true;
                        app.quit = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.select_next_visible_item(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_previous_visible_item(),
                    KeyCode::Char(' ') | KeyCode::Enter => app.toggle_current_item_selection(),
                    KeyCode::Char('o') | KeyCode::Tab => {
                        app.toggle_expansion_and_adjust_selection()
                    }

                    KeyCode::Char('*') => {
                        // Shift+8 on US QWERTY for Expand All
                        app.expand_all_directories();
                    }
                    KeyCode::Char('-') => {
                        // Hyphen/Minus for Collapse All
                        app.collapse_all_directories();
                    }
                    KeyCode::Char('a') => {
                        // 'a' for Select All Visible Files
                        app.select_all_visible_items();
                    }
                    KeyCode::Char('d') => {
                        // Ensure no modifiers for this action
                        app.deselect_all_visible_items();
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn ui_frame(frame: &mut Frame, app: &mut TuiApp) {
    let help_lines = 2;
    let filter_input_height = if app.mode == AppMode::Filtering { 3 } else { 0 }; // Block with borders
    let top_block_height = (help_lines + 2) + filter_input_height; // +2 for help block borders
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(top_block_height), // Combined help and filter
            Constraint::Min(0),                   // List
        ])
        .split(frame.area());

    // Split the top area for help and filter
    let top_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if app.mode == AppMode::Filtering {
            vec![
                Constraint::Length(help_lines + 2),
                Constraint::Length(filter_input_height),
            ]
        } else {
            vec![Constraint::Length(help_lines + 2)]
        })
        .split(chunks[0]);

    // Render Help Text
    let help_text_lines_content = vec![
        Line::from("Arrows/jk: Nav | Space/Enter: Sel | Tab/o: Fold | y: Confirm | q/Esc: Quit"),
        Line::from("a: Sel All Vis | d: Desel All | *: Expand All | -: Collapse All | /: Filter"),
    ];
    let help_paragraph = Paragraph::new(help_text_lines_content).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Repoyank Interactive Selection"),
    );
    frame.render_widget(help_paragraph, top_chunks[0]);

    // Render Filter Input if in Filtering mode
    if app.mode == AppMode::Filtering {
        let input_text = format!("/{}", app.filter_input);
        let filter_paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Filter (Esc to cancel, Enter to apply)"),
            )
            .wrap(Wrap { trim: false }); // Show overflowing text
        frame.render_widget(filter_paragraph, top_chunks[1]);
        // Show cursor in filter input
        frame.set_cursor_position((
            top_chunks[1].x + 1 + app.filter_cursor_pos as u16 + 1, // +1 for border, +1 for '/'
            top_chunks[1].y + 1,                                    // +1 for border
        ));
    } else {
    }

    // --- List Rendering ---
    // Ensure selection is valid before getting visible items for rendering
    // This handles cases where filter changes and selection might become invalid.
    // It's better to call ensure_selection_is_valid_after_filter() in handle_events after filter changes.
    // Here, we mostly focus on scrolling for the current valid selection.
    app.ensure_selection_is_visible_in_viewport(); // This now uses a placeholder for list_height

    let visible_item_indices = app.get_visible_item_indices(); // This is now filter-aware
    let num_visible_items = visible_item_indices.len();
    let list_area = chunks[1]; // Use the correct chunk for the list
    let list_height_for_scroll_calc = list_area.height.saturating_sub(2) as usize; // Actual list height

    // This logic is now partly duplicated in ensure_selection_is_visible_in_viewport. Consolidate if possible.
    // For now, let's assume ensure_selection_is_visible_in_viewport is called with correct list_height or TuiApp stores it.
    // Simplified scroll adjustment (assuming ensure_selection_is_visible_in_viewport has a similar logic)
    let current_selected_item_in_visible_list_idx_opt = visible_item_indices
        .iter()
        .position(|&idx| idx == app.current_selection_idx);

    if let Some(selected_idx_in_visible_list) = current_selected_item_in_visible_list_idx_opt {
        if selected_idx_in_visible_list < app.scroll_offset {
            app.scroll_offset = selected_idx_in_visible_list;
        } else if selected_idx_in_visible_list >= app.scroll_offset + list_height_for_scroll_calc {
            app.scroll_offset = selected_idx_in_visible_list - list_height_for_scroll_calc + 1;
        }
    }

    let list_items_to_render_indices = visible_item_indices
        .get(
            app.scroll_offset
                ..(app.scroll_offset + list_height_for_scroll_calc).min(num_visible_items),
        )
        .unwrap_or(&[]);

    let list_items: Vec<ListItem> = list_items_to_render_indices
        .iter()
        .map(|&item_actual_idx| {
            let item = &app.items[item_actual_idx];
            let selection_prefix = match item.state {
                SelectionState::NotSelected => "[ ] ",
                SelectionState::PartiallySelected => "[-] ",
                SelectionState::FullySelected => "[x] ",
            };
            let expansion_prefix = if item.is_dir {
                if item.is_expanded { "[-] " } else { "[+] " }
            } else {
                "    "
            };
            let full_line = format!(
                "{}{}{}",
                expansion_prefix, selection_prefix, item.display_text
            );
            ListItem::new(full_line)
        })
        .collect();

    let list_widget = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(
            if !app.filter_input.is_empty() && app.mode == AppMode::Normal {
                // Show filter in title if active and normal mode
                format!("Files (Filter: '{}')", app.filter_input)
            } else {
                "Select files/directories".to_string()
            },
        ))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("❯ ");

    // Create a ListState for rendering the visible part
    let mut list_state_for_view = ratatui::widgets::ListState::default();
    if let Some(selected_idx_in_visible_list) = current_selected_item_in_visible_list_idx_opt {
        if selected_idx_in_visible_list >= app.scroll_offset
            && selected_idx_in_visible_list < app.scroll_offset + list_height_for_scroll_calc
        {
            list_state_for_view.select(Some(selected_idx_in_visible_list - app.scroll_offset));
        }
    }

    frame.render_stateful_widget(list_widget, list_area, &mut list_state_for_view);
}
