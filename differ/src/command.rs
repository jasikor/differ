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
    /// Move focus to the next region left / right.
    FocusLeft,
    FocusRight,
    /// Move focus between the left panel's sections (versions / files).
    FocusUp,
    FocusDown,
    /// Put the focused pane's cursor line at the middle / top / bottom.
    CursorCenter,
    CursorTop,
    CursorBottom,
    /// Scroll the focused pane's view one line down / up.
    ScrollDown,
    ScrollUp,
    /// Zoom the focused diff pane to the full width (toggle).
    ToggleZoom,
    /// Show/hide the left panel (changed files + versions).
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
    /// Focus the versions list in the left panel.
    CompareRevisions,
    /// Compare the highlighted version (from the marked base, if any).
    ApplyRevision,
    /// Mark/unmark the highlighted version as the base of a comparison.
    MarkRevision,
    /// Type a revision spec instead of picking from the versions list.
    TypeRevision,
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
            Command::FocusLeft => app.focus_left(),
            Command::FocusRight => app.focus_right(),
            Command::FocusUp => app.focus_up(),
            Command::FocusDown => app.focus_down(),
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
            Command::CompareRevisions => app.focus_revisions(),
            Command::ApplyRevision => app.apply_revision(),
            Command::MarkRevision => app.mark_revision(),
            Command::TypeRevision => app.start_range_prompt(),
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
            Command::FocusLeft => "focus left",
            Command::FocusRight => "focus right",
            Command::FocusUp => "focus up",
            Command::FocusDown => "focus down",
            Command::CursorCenter => "cursor to middle",
            Command::CursorTop => "cursor to top",
            Command::CursorBottom => "cursor to bottom",
            Command::ScrollDown => "scroll view down",
            Command::ScrollUp => "scroll view up",
            Command::ToggleZoom => "zoom pane",
            Command::ToggleFiles => "toggle changes/versions panel",
            Command::StartJump => "jump (flash)",
            Command::Search => "search",
            Command::SearchNext => "next match",
            Command::SearchPrev => "previous match",
            Command::GotoDefinition => "go to definition",
            Command::GotoReferences => "go to references",
            Command::Back => "back",
            Command::CompareRevisions => "focus versions",
            Command::ApplyRevision => "compare revision",
            Command::MarkRevision => "mark base revision",
            Command::TypeRevision => "type a revision range",
            Command::NextFile => "next file",
            Command::PrevFile => "previous file",
            Command::Help => "help",
            Command::Reload => "reload",
        }
    }
}
