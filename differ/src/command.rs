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
    /// Move the cursor one column left / right in the focused pane.
    Left,
    Right,
    PageDown,
    PageUp,
    /// Jump to the top of the focused region.
    Top,
    /// Jump to the bottom of the focused region.
    Bottom,
    /// Move the cursor to the start / first non-blank / end of the line.
    LineStart,
    LineFirstNonBlank,
    LineEnd,
    /// Move the cursor by words (vim's `w`/`b`/`e`).
    WordForward,
    WordBack,
    WordEnd,
    /// Move by WORDs (vim's `W`/`B`/`E`): whitespace-delimited.
    WordForwardBig,
    WordBackBig,
    WordEndBig,
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
    /// Show/hide the file (git) panel.
    ToggleFiles,
    /// Start the `s` jump: type a character, then a label to teleport there.
    StartJump,
    /// Open the `/` search prompt.
    Search,
    /// Jump to the next / previous search match.
    SearchNext,
    SearchPrev,
    /// Go to the definition of the symbol under the cursor (ripgrep-based).
    GotoDefinition,
    /// List the references to the symbol under the cursor.
    GotoReferences,
    /// Return from a symbol jump.
    Back,
    /// Select the next / previous file, whatever has focus.
    NextFile,
    PrevFile,
    /// Toggle the full-screen key help.
    Help,
    Reload,
}

impl Command {
    pub fn run(self, app: &mut App) -> Outcome {
        match self {
            Command::Quit => return Outcome::Quit,
            Command::Down => app.down(),
            Command::Up => app.up(),
            Command::Left => app.col_left(),
            Command::Right => app.col_right(),
            Command::PageDown => app.page_down(),
            Command::PageUp => app.page_up(),
            Command::Top => app.top(),
            Command::Bottom => app.bottom(),
            Command::LineStart => app.line_start(),
            Command::LineFirstNonBlank => app.line_first_non_blank(),
            Command::LineEnd => app.line_end(),
            Command::WordForward => app.word_forward(),
            Command::WordBack => app.word_back(),
            Command::WordEnd => app.word_end(),
            Command::WordForwardBig => app.word_forward_big(),
            Command::WordBackBig => app.word_back_big(),
            Command::WordEndBig => app.word_end_big(),
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
            Command::ToggleFiles => app.toggle_files(),
            Command::StartJump => app.start_jump(),
            Command::Search => app.start_search(),
            Command::SearchNext => app.search_next(),
            Command::SearchPrev => app.search_prev(),
            Command::GotoDefinition => app.goto_definition(),
            Command::GotoReferences => app.goto_references(),
            Command::Back => app.go_back(),
            Command::NextFile => app.next_file(),
            Command::PrevFile => app.prev_file(),
            Command::Help => app.toggle_help(),
            Command::Reload => app.refresh(),
        }
        Outcome::Continue
    }

    /// A short description, for the which-key hint and the help page.
    pub fn describe(self) -> &'static str {
        match self {
            Command::Quit => "quit",
            Command::Down => "down",
            Command::Up => "up",
            Command::Left => "left",
            Command::Right => "right",
            Command::PageDown => "page down",
            Command::PageUp => "page up",
            Command::Top => "top",
            Command::Bottom => "bottom",
            Command::LineStart => "line start",
            Command::LineFirstNonBlank => "first non-blank",
            Command::LineEnd => "line end",
            Command::WordForward => "word forward",
            Command::WordBack => "word back",
            Command::WordEnd => "word end",
            Command::WordForwardBig => "WORD forward",
            Command::WordBackBig => "WORD back",
            Command::WordEndBig => "WORD end",
            Command::NextHunk => "next change",
            Command::PrevHunk => "previous change",
            Command::FocusPrev => "focus previous panel",
            Command::FocusNext => "focus next panel",
            Command::CursorCenter => "cursor to middle",
            Command::CursorTop => "cursor to top",
            Command::CursorBottom => "cursor to bottom",
            Command::ScrollDown => "scroll view down",
            Command::ScrollUp => "scroll view up",
            Command::ToggleZoom => "zoom pane",
            Command::ToggleFiles => "toggle file panel",
            Command::StartJump => "jump (flash)",
            Command::Search => "search",
            Command::SearchNext => "next match",
            Command::SearchPrev => "previous match",
            Command::GotoDefinition => "go to definition",
            Command::GotoReferences => "go to references",
            Command::Back => "back",
            Command::NextFile => "next file",
            Command::PrevFile => "previous file",
            Command::Help => "help",
            Command::Reload => "reload",
        }
    }
}
