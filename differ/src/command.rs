//! Commands: the actions, decoupled from the keys that trigger them.
//!
//! Adding/renaming a command happens here; wiring it to keys happens in
//! `keymap.rs`. Neither touches the event loop. Most commands act on whatever
//! region has focus (file list vs. one of the two panes).

use crate::app::App;

/// What the event loop should do after a command runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Continue,
    Quit,
}

/// A user-triggerable action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Quit,
    /// Move one step down in the focused region.
    Down,
    /// Move one step up in the focused region.
    Up,
    PageDown,
    PageUp,
    /// Jump to the top of the focused region.
    Top,
    NextHunk,
    PrevHunk,
    /// Cycle focus to the previous region.
    FocusPrev,
    /// Cycle focus to the next region.
    FocusNext,
    /// Put the focused pane's cursor line at the middle / top / bottom.
    CursorCenter,
    CursorTop,
    CursorBottom,
    /// Scroll the focused pane's view one line down / up.
    ScrollDown,
    ScrollUp,
    /// Zoom the focused diff pane to the full width (toggle).
    ToggleZoom,
    Reload,
}

impl Command {
    pub fn run(self, app: &mut App) -> Outcome {
        match self {
            Command::Quit => return Outcome::Quit,
            Command::Down => app.down(),
            Command::Up => app.up(),
            Command::PageDown => app.page_down(),
            Command::PageUp => app.page_up(),
            Command::Top => app.top(),
            Command::NextHunk => app.next_hunk(),
            Command::PrevHunk => app.prev_hunk(),
            Command::FocusPrev => app.focus_prev(),
            Command::FocusNext => app.focus_next(),
            Command::CursorCenter => app.cursor_center(),
            Command::CursorTop => app.cursor_top(),
            Command::CursorBottom => app.cursor_bottom(),
            Command::ScrollDown => app.scroll_down(),
            Command::ScrollUp => app.scroll_up(),
            Command::ToggleZoom => app.toggle_zoom(),
            Command::Reload => app.refresh(),
        }
        Outcome::Continue
    }
}
