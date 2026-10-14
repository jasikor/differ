//! Commands: the actions, decoupled from the keys that trigger them.
//!
//! Adding/renaming a command happens here; wiring it to keys happens in
//! `keymap.rs`. Neither touches the event loop. Most commands act on whatever
//! region has focus (file list vs. one of the two panes).

use crate::app::{App, Focus};

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
    /// Focus a specific region (the number keys).
    Focus(Focus),
    /// Cycle focus to the previous region.
    FocusPrev,
    /// Cycle focus to the next region.
    FocusNext,
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
            Command::Focus(focus) => app.set_focus(focus),
            Command::FocusPrev => app.focus_prev(),
            Command::FocusNext => app.focus_next(),
            Command::Reload => app.refresh(),
        }
        Outcome::Continue
    }
}
