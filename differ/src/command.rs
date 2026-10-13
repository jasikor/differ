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
    /// Cycle focus between the file list and the two panes.
    ToggleFocus,
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
            Command::ToggleFocus => app.toggle_focus(),
            Command::Reload => app.refresh(),
        }
        Outcome::Continue
    }
}
