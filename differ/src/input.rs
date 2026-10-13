//! Keyboard handling. Returns `true` when the app should quit.

use ratatui::crossterm::event::KeyCode;

use crate::app::App;

pub fn handle(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
        KeyCode::Char('J') => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Char('K') => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::PageDown => app.scroll = app.scroll.saturating_add(20),
        KeyCode::PageUp => app.scroll = app.scroll.saturating_sub(20),
        KeyCode::Home | KeyCode::Char('g') => app.scroll = 0,
        KeyCode::Char('.') => app.opts.delta += 1,
        KeyCode::Char(',') => app.opts.delta -= 1,
        KeyCode::Char('R') => app.refresh(),
        _ => {}
    }
    false
}
