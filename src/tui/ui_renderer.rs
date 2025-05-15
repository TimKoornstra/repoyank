use super::app_logic::TuiApp;
use super::app_state::AppMode;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

fn draw_help_block(f: &mut Frame, _app: &TuiApp, area: Rect) {
    let help_text_lines_content = vec![
        Line::from("Arrows/jk: Nav | Space/Enter: Sel | Tab/o: Fold | y: Confirm | q/Esc: Quit"),
        Line::from("a: Sel All Vis | d: Desel All | *: Expand All | -: Collapse All | /: Filter"),
    ];
    let help_paragraph = Paragraph::new(help_text_lines_content).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Repoyank Interactive Selection"),
    );
    f.render_widget(help_paragraph, area);
}

fn draw_filter_input_block(f: &mut Frame, app: &TuiApp, area: Rect) {
    let input_text = format!("/{}", app.filter_input);
    let filter_paragraph = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Filter (Esc to cancel, Enter to apply)"),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(filter_paragraph, area);
    f.set_cursor_position((area.x + 1 + app.filter_cursor_pos as u16 + 1, area.y + 1));
}

fn draw_main_list_block(f: &mut Frame, app: &mut TuiApp, area: Rect) {
    app.list_viewport_height = area.height.saturating_sub(2) as usize;
    app.ensure_selection_is_visible_in_viewport(); // Call this to adjust scroll based on current state

    let visible_item_indices = app.get_visible_item_indices();
    let num_visible_items = visible_item_indices.len();

    let list_items_to_render_indices = visible_item_indices
        .get(
            app.scroll_offset
                ..(app.scroll_offset + app.list_viewport_height).min(num_visible_items),
        )
        .unwrap_or(&[]);

    let list_items: Vec<ListItem> = list_items_to_render_indices
        .iter()
        .map(|&item_actual_idx| {
            let item = &app.items[item_actual_idx];
            let selection_prefix = match item.state {
                super::app_state::SelectionState::NotSelected => "[ ] ",
                super::app_state::SelectionState::PartiallySelected => "[-] ",
                super::app_state::SelectionState::FullySelected => "[x] ",
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

    let list_title = if !app.filter_input.is_empty() && app.mode == AppMode::Normal {
        format!("Files (Filter: '{}')", app.filter_input)
    } else {
        "Select files/directories".to_string()
    };

    let list_widget = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("❯ ");

    let mut list_state_for_view = ratatui::widgets::ListState::default();
    let current_selected_item_in_visible_list_idx_opt = visible_item_indices
        .iter()
        .position(|&idx| idx == app.current_selection_idx);

    if let Some(selected_idx_in_visible_list) = current_selected_item_in_visible_list_idx_opt {
        if selected_idx_in_visible_list >= app.scroll_offset
            && selected_idx_in_visible_list < app.scroll_offset + app.list_viewport_height
        {
            list_state_for_view.select(Some(selected_idx_in_visible_list - app.scroll_offset));
        }
    }
    f.render_stateful_widget(list_widget, area, &mut list_state_for_view);
}

pub(super) fn ui_frame(frame: &mut Frame, app: &mut TuiApp) {
    let help_lines = 2;
    let filter_input_height = if app.mode == AppMode::Filtering { 3 } else { 0 };
    let top_block_container_height = (help_lines + 2) + filter_input_height;

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(top_block_container_height),
            Constraint::Min(0),
        ])
        .split(frame.area());

    let top_container_area = main_chunks[0];
    let list_area = main_chunks[1];

    let top_content_constraints = if app.mode == AppMode::Filtering {
        vec![
            Constraint::Length(help_lines + 2),
            Constraint::Length(filter_input_height),
        ]
    } else {
        vec![Constraint::Length(help_lines + 2)]
    };
    let top_content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(top_content_constraints)
        .split(top_container_area);

    draw_help_block(frame, app, top_content_chunks[0]);
    if app.mode == AppMode::Filtering {
        draw_filter_input_block(frame, app, top_content_chunks[1]);
    }

    draw_main_list_block(frame, app, list_area);
}
