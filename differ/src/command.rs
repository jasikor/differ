//! Commands: the actions, decoupled from the keys that trigger them.
//!
//! Adding/renaming a command happens here; wiring it to keys happens in
//! `keymap.rs`. Neither touches the event loop.

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
    NextFile,
    PrevFile,
    NextHunk,
    PrevHunk,
    ScrollDown,
    ScrollUp,
    PageDown,
    PageUp,
    Top,
    OffsetRight,
    OffsetLeft,
    Reload,
}

impl Command {
    pub fn run(self, app: &mut App) -> Outcome {
        match self {
            Command::Quit => return Outcome::Quit,
            Command::NextFile => app.move_selection(1),
            Command::PrevFile => app.move_selection(-1),
            Command::NextHunk => app.next_hunk(),
            Command::PrevHunk => app.prev_hunk(),
            Command::ScrollDown => app.scroll = app.scroll.saturating_add(1),
            Command::ScrollUp => app.scroll = app.scroll.saturating_sub(1),
            Command::PageDown => app.scroll = app.scroll.saturating_add(20),
            Command::PageUp => app.scroll = app.scroll.saturating_sub(20),
            Command::Top => app.scroll = 0,
            Command::OffsetRight => app.opts.delta += 1,
            Command::OffsetLeft => app.opts.delta -= 1,
            Command::Reload => app.refresh(),
        }
        Outcome::Continue
    }
}
