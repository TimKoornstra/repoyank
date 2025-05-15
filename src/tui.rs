use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
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
}

impl TuiApp {
    fn new(items: Vec<SelectableItem>) -> Self {
        TuiApp {
            items,
            current_selection_idx: 0,
            scroll_offset: 0,
            quit: false,
            confirmed: false,
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
        for i in 0..self.items.len() {
            if self.is_item_visible(i) {
                visible_indices.push(i);
            }
        }
        visible_indices
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
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                // Handle primary character key presses (no complex modifiers expected for these actions)
                match key_event.code {
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
    let top_block_height = 4;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(top_block_height), Constraint::Min(0)])
        .split(frame.area());

    let help_text_lines = vec![
        Line::from("Arrows/jk: Nav | Space/Enter: Sel | Tab/o: Fold | y: Confirm | q/Esc: Quit"),
        Line::from("a: Select All Visible | d: Deselect All | *: Expand All | -: Collapse All"),
    ];
    let help_paragraph = Paragraph::new(help_text_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Repoyank Interactive Selection"),
    );
    frame.render_widget(help_paragraph, layout[0]);

    app.ensure_selection_is_visible();
    let visible_item_indices = app.get_visible_item_indices();
    let num_visible_items = visible_item_indices.len();
    let list_height = layout[1].height.saturating_sub(2) as usize;

    let current_selected_item_in_visible_list_idx_opt = visible_item_indices
        .iter()
        .position(|&idx| idx == app.current_selection_idx);

    if let Some(selected_idx_in_visible_list) = current_selected_item_in_visible_list_idx_opt {
        if selected_idx_in_visible_list < app.scroll_offset {
            app.scroll_offset = selected_idx_in_visible_list;
        } else if selected_idx_in_visible_list >= app.scroll_offset + list_height {
            app.scroll_offset = selected_idx_in_visible_list - list_height + 1;
        }
    } else if num_visible_items > 0 {
        app.scroll_offset = 0;
    }

    if num_visible_items == 0 {
        app.scroll_offset = 0;
    } else if num_visible_items < list_height {
        app.scroll_offset = 0;
    } else if app.scroll_offset > num_visible_items.saturating_sub(list_height) {
        app.scroll_offset = num_visible_items.saturating_sub(list_height);
    }
    app.scroll_offset = app.scroll_offset.max(0);
    if num_visible_items > 0 {
        app.scroll_offset = app.scroll_offset.min(num_visible_items.saturating_sub(1));
    }

    let visible_indices_to_render_slice = visible_item_indices
        .get(app.scroll_offset..(app.scroll_offset + list_height).min(num_visible_items))
        .unwrap_or(&[]);

    let list_items: Vec<ListItem> = visible_indices_to_render_slice
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

            // item.display_text contains the tree structure (e.g., "├─ dirname/")
            // The full line is now: [Expansion] [Selection] TreeLabel
            let full_line = format!(
                "{}{}{}",
                expansion_prefix, selection_prefix, item.display_text
            );
            ListItem::new(full_line)
        })
        .collect();

    let list_widget = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Select files/directories",)),
        )
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
            && selected_idx_in_visible_list < app.scroll_offset + list_height
        {
            list_state_for_view.select(Some(selected_idx_in_visible_list - app.scroll_offset));
        }
    }

    frame.render_stateful_widget(list_widget, layout[1], &mut list_state_for_view);
}
