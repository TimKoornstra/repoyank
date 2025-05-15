use super::app_logic::TuiApp;
use super::app_state::AppMode;
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

pub(super) fn handle_events(app: &mut TuiApp) -> Result<()> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                match app.mode {
                    AppMode::Normal => app.handle_normal_mode_input(key_event),
                    AppMode::Filtering => app.handle_filtering_mode_input(key_event),
                }
            }
        }
    }
    Ok(())
}
