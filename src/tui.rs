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
    pub state: SelectionState,
    // For propagating selections:
    pub children_indices: Vec<usize>,
    pub parent_index: Option<usize>,
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

    fn next_item(&mut self) {
        if !self.items.is_empty() {
            self.current_selection_idx = (self.current_selection_idx + 1) % self.items.len();
        }
    }

    fn prev_item(&mut self) {
        if !self.items.is_empty() {
            if self.current_selection_idx == 0 {
                self.current_selection_idx = self.items.len() - 1;
            } else {
                self.current_selection_idx -= 1;
            }
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

        self.apply_state_and_propagate_down(item_idx, new_state_for_item);
        self.update_all_parent_states_from_child(item_idx);
    }

    fn apply_state_and_propagate_down(&mut self, item_idx: usize, new_state: SelectionState) {
        let actual_new_state =
            if !self.items[item_idx].is_dir && new_state == SelectionState::PartiallySelected {
                SelectionState::FullySelected // Files cannot be PartiallySelected
            } else {
                new_state
            };

        self.items[item_idx].state = actual_new_state;

        if self.items[item_idx].is_dir && actual_new_state != SelectionState::PartiallySelected {
            let children_indices = self.items[item_idx].children_indices.clone();
            for child_idx in children_indices {
                self.apply_state_and_propagate_down(child_idx, actual_new_state);
            }
        }
    }

    fn update_all_parent_states_from_child(&mut self, child_idx: usize) {
        let mut current_parent_idx_opt = self.items[child_idx].parent_index;
        while let Some(parent_idx) = current_parent_idx_opt {
            self.recalculate_parent_state(parent_idx);
            current_parent_idx_opt = self.items[parent_idx].parent_index;
        }
    }

    fn recalculate_parent_state(&mut self, parent_idx: usize) {
        if !self.items[parent_idx].is_dir {
            return;
        }

        let children_indices = self.items[parent_idx].children_indices.clone();
        if children_indices.is_empty() {
            // If a dir has no selectable children, its state is only changed by direct interaction
            // or propagation from its parent. This function does not change its state.
            return;
        }

        let mut num_fully_selected_children = 0;
        let mut num_partially_selected_children = 0;

        for &child_idx in &children_indices {
            match self.items[child_idx].state {
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

        self.items[parent_idx].state = new_parent_state;
    }
}

fn prepare_selectable_items(
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
            state: SelectionState::NotSelected,
            children_indices: Vec::new(),
            parent_index: None,
        });
    }

    for i in 0..selectable_items.len() {
        let path = selectable_items[i].path.clone();
        if path != *root_path {
            // Root item has no parent in this context
            if let Some(parent_pbuf) = path.parent() {
                // Ensure parent_path is something that would be in path_to_idx_map
                // (i.e., it's within root_path and was part of selectable_items)
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

pub fn run_tui(
    initial_items_paths_is_dir: &[(PathBuf, bool)],
    display_labels: &[String],
    root_path: &Path,
) -> Result<Option<Vec<SelectableItem>>> {
    if initial_items_paths_is_dir.is_empty() {
        // No items were even presented to TUI, treat as if user selected nothing confirmable.
        // Or, if we want to distinguish "nothing to select" from "cancelled empty list",
        // this could return Ok(Some(Vec::new())) if we want main to handle it.
        // For now, let's assume if it's empty, it's like cancelling an empty selection.
        return Ok(None);
    }

    let prepared_items =
        prepare_selectable_items(initial_items_paths_is_dir, display_labels, root_path);
    if prepared_items.is_empty() {
        // Should ideally not happen if initial_items_paths_is_dir wasn't empty
        return Ok(None);
    }
    let mut app = TuiApp::new(prepared_items);

    let mut terminal = init_terminal()?;

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
    // Poll for an event with a timeout.
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // Process only key presses
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
                    KeyCode::Enter => {
                        app.confirmed = true;
                        app.quit = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.next_item(),
                    KeyCode::Up | KeyCode::Char('k') => app.prev_item(),
                    KeyCode::Char(' ') => app.toggle_current_item_selection(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn ui_frame(frame: &mut Frame, app: &mut TuiApp) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Title/Help
            Constraint::Min(0),    // List
        ])
        .split(frame.size());

    let help_text = "Arrows/jk: Navigate | Space: Toggle | Enter: Confirm | q/Esc: Quit";
    let help_paragraph = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Repoyank Interactive Selection"),
    );
    frame.render_widget(help_paragraph, layout[0]);

    let list_height = layout[1].height.saturating_sub(2) as usize; // Account for borders

    // Adjust scroll_offset to keep current_selection_idx visible
    if app.current_selection_idx < app.scroll_offset {
        app.scroll_offset = app.current_selection_idx;
    } else if app.current_selection_idx >= app.scroll_offset + list_height {
        app.scroll_offset = app.current_selection_idx - list_height + 1;
    }
    // Ensure scroll_offset is not out of bounds if list is smaller than height
    if app.items.len() < list_height {
        app.scroll_offset = 0;
    } else if app.scroll_offset > app.items.len().saturating_sub(list_height) {
        app.scroll_offset = app.items.len().saturating_sub(list_height);
    }

    let visible_items_end_idx = (app.scroll_offset + list_height).min(app.items.len());
    let items_to_display_slice = if app.scroll_offset < visible_items_end_idx {
        &app.items[app.scroll_offset..visible_items_end_idx]
    } else {
        &[] // Should not happen if logic is correct
    };

    let list_items: Vec<ListItem> = items_to_display_slice
        .iter()
        .map(|selectable_item| {
            let prefix = match selectable_item.state {
                SelectionState::NotSelected => "[ ] ",
                SelectionState::PartiallySelected => "[-] ",
                SelectionState::FullySelected => "[x] ",
            };
            let line = format!("{}{}", prefix, selectable_item.display_text);
            ListItem::new(line)
        })
        .collect();

    let list_widget = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select files/directories"),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("❯ ");

    // Create a ListState for rendering the visible part
    let mut list_state_for_view = ratatui::widgets::ListState::default();
    // The selected item in the view is current_selection_idx - scroll_offset
    if app.current_selection_idx >= app.scroll_offset
        && app.current_selection_idx < visible_items_end_idx
    {
        list_state_for_view.select(Some(app.current_selection_idx - app.scroll_offset));
    }

    frame.render_stateful_widget(list_widget, layout[1], &mut list_state_for_view);
}
