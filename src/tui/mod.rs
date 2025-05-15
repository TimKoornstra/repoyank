// Define structs and enums that are part of the public API of the TUI module
mod app_logic;
mod app_state;
mod event_handler;
mod ui_renderer;

// Re-export necessary items for use by other modules (e.g., workflow.rs)
pub use app_state::{SelectableItem, SelectionState};
// TuiApp itself is not directly used by workflow.rs, but its `new` method is used by run_tui.
// The propagation helpers and prepare_selectable_items are directly used by workflow.
pub use app_logic::{
    apply_state_and_propagate_down_vec, prepare_selectable_items,
    update_all_parent_states_from_child_vec,
};

// The main function to run the TUI
pub use self::run_tui::run_tui_with_prepared_items;

// This module will contain the main TUI loop and terminal setup/teardown
mod run_tui {
    use super::app_logic::TuiApp;
    use super::app_state::SelectableItem;
    use super::event_handler::handle_events;
    use super::ui_renderer::ui_frame;
    use anyhow::Result;
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::prelude::{CrosstermBackend, Terminal};
    use std::io::{self, Stdout};
    use std::path::Path;

    pub fn run_tui_with_prepared_items(
        prepared_items: Vec<SelectableItem>,
        #[allow(unused_variables)] root_path: &Path,
    ) -> Result<Option<Vec<SelectableItem>>> {
        if prepared_items.is_empty() {
            return Ok(None);
        }
        let mut app = TuiApp::new(prepared_items);

        let mut terminal = init_terminal()?;
        // Initial call to set up viewport height and ensure selection is visible
        // This is a bit of a chicken-and-egg: draw to get height, then adjust.
        // A dummy draw or initial height assumption might be needed if this causes issues.
        // For now, ensure_selection_is_visible will use list_viewport_height=0 initially.
        app.ensure_selection_is_visible();

        while !app.quit {
            // app.quit is pub(super)
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
}
