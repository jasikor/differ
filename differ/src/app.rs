//! Application state, independent of rendering.

use std::collections::HashMap;

use ratatui::crossterm::event::KeyCode;

use crate::git::{self, Repo};
use crate::model::{self, DiffRow};
use crate::tree;

/// Context lines kept above a jumped-to hunk.
const HUNK_CONTEXT: usize = 3;

/// Which region receives movement keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Files,
    Old,
    New,
}

impl Focus {
    /// The region one step to the right (stops at the rightmost pane).
    fn right(self) -> Self {
        match self {
            Focus::Files => Focus::Old,
            Focus::Old => Focus::New,
            Focus::New => Focus::New,
        }
    }

    /// The region one step to the left (stops at the file list).
    fn left(self) -> Self {
        match self {
            Focus::Files => Focus::Files,
            Focus::Old => Focus::Files,
            Focus::New => Focus::Old,
        }
    }
}

/// Remembered view position for a file, so reopening it keeps its place.
#[derive(Clone, Copy)]
struct ViewState {
    old_cursor: usize,
    new_cursor: usize,
    old_top: usize,
    new_top: usize,
}

/// The `s` jump: press it, type a pattern, then a label to teleport there.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JumpState {
    #[default]
    Idle,
    /// Typing a pattern / choosing a label (flash's jump mode).
    Active,
}

/// One visible occurrence of the jump target, with its label and position.
#[derive(Clone, Debug, PartialEq)]
pub struct JumpMatch {
    pub old_side: bool,
    /// Side line index (the pane's own line numbering).
    pub line: usize,
    /// Character column within the line.
    pub col: usize,
    /// The single-character label, absent when there were more matches than
    /// labels (flash leaves those unlabelled).
    pub label: Option<char>,
}

/// One occurrence of the active search pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub line: usize,
    pub col: usize,
    /// Length in characters.
    pub len: usize,
}

/// The active `/` search: a literal pattern (smartcase) over the whole focused
/// side, plus its cached matches.
#[derive(Clone, Debug, PartialEq)]
pub struct Search {
    pub old_side: bool,
    pub pattern: String,
    pub matches: Vec<SearchHit>,
}

pub struct App {
    pub repo: Option<Repo>,
    pub files: Vec<git::FileEntry>,
    /// Sidebar rows (directories + files) built from `files`, and the row each
    /// file occupies so movement can skip the directory labels.
    pub tree: Vec<tree::Row>,
    pub file_row: Vec<usize>,
    pub selected: usize,
    pub file_scroll: u16,
    pub rows: Vec<DiffRow>,
    /// Top old line visible.
    pub old_top: usize,
    /// Top new line visible.
    pub new_top: usize,
    /// Cursor line in each pane (kept paired by the line maps).
    pub old_cursor: usize,
    pub new_cursor: usize,
    /// Goal cursor column in each pane (vim's curswant): the desired column,
    /// clamped to the line for drawing and motion.
    old_col: usize,
    new_col: usize,
    /// Horizontal scroll offset (code columns) per pane, kept so the cursor
    /// column stays visible.
    old_left: usize,
    new_left: usize,
    pub focus: Focus,
    /// When set, that diff pane fills the width and the other pane, gutter and
    /// its ruler are hidden.
    pub zoom: Option<Focus>,
    /// Whether the file (git) panel is shown. Hidden, the diff panes take the
    /// full width and the panel drops out of the `Ctrl-h`/`Ctrl-l` cycle.
    pub show_files: bool,
    /// The most recent diff pane focused, returned to when the panel is hidden.
    last_diff: Focus,
    /// Pane body height, refreshed by the view each frame.
    pub viewport: usize,
    /// Visible code width per pane, refreshed by the view each frame.
    pub old_code_w: usize,
    pub new_code_w: usize,
    new_of_old: Vec<usize>,
    old_of_new: Vec<usize>,
    /// Saved view position per file path.
    view_state: HashMap<String, ViewState>,
    /// The `s` jump state and its matches.
    jump: JumpState,
    jump_pattern: String,
    jump_matches: Vec<JumpMatch>,
    /// The active `/` search (if any) and whether its pattern is being typed.
    search: Option<Search>,
    search_input: bool,
    /// Where the search started, to preview around and to restore on cancel.
    search_origin: (bool, usize, usize),
    pub message: String,
}

impl App {
    pub fn new(start: Option<&std::path::Path>) -> Self {
        let mut app = App {
            repo: None,
            files: Vec::new(),
            tree: Vec::new(),
            file_row: Vec::new(),
            selected: 0,
            file_scroll: 0,
            rows: Vec::new(),
            old_top: 0,
            new_top: 0,
            old_cursor: 0,
            new_cursor: 0,
            old_col: 0,
            new_col: 0,
            old_left: 0,
            new_left: 0,
            focus: Focus::Files,
            zoom: None,
            show_files: true,
            last_diff: Focus::Old,
            viewport: 1,
            old_code_w: 1,
            new_code_w: 1,
            new_of_old: Vec::new(),
            old_of_new: Vec::new(),
            view_state: HashMap::new(),
            jump: JumpState::Idle,
            jump_pattern: String::new(),
            jump_matches: Vec::new(),
            search: None,
            search_input: false,
            search_origin: (true, 0, 0),
            message: String::new(),
        };
        match Repo::discover_in(start) {
            Ok(repo) => {
                app.repo = Some(repo);
                app.refresh();
            }
            Err(e) => app.message = format!("not a git repository: {e}"),
        }
        app
    }

    /// Re-read the file list and the selected diff.
    pub fn refresh(&mut self) {
        let Some(repo) = &self.repo else { return };
        match repo.status() {
            Ok(mut files) => {
                // Sidebar order: subdirectories before files, recursively.
                tree::order(&mut files);
                self.files = files;
                if self.selected >= self.files.len() {
                    self.selected = self.files.len().saturating_sub(1);
                }
            }
            Err(e) => self.message = format!("status: {e}"),
        }
        self.rebuild_tree();
        self.load_diff();
    }

    fn rebuild_tree(&mut self) {
        let (rows, file_row) = tree::build(&self.files);
        self.tree = rows;
        self.file_row = file_row;
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.files.get(self.selected).map(|f| f.path.as_str())
    }

    /// How far the new-side rows are offset from the old-view gutter grid.
    pub fn pair_shift(&self) -> i32 {
        self.old_top as i32 - self.new_top as i32
    }

    fn old_total(&self) -> usize {
        self.new_of_old.len()
    }

    fn new_total(&self) -> usize {
        self.old_of_new.len()
    }

    // ---- movement (acts on the focused region) ----

    pub fn down(&mut self) {
        self.step(1);
    }

    pub fn up(&mut self) {
        self.step(-1);
    }

    pub fn page_down(&mut self) {
        self.step(self.viewport.max(1) as isize);
    }

    pub fn page_up(&mut self) {
        self.step(-(self.viewport.max(1) as isize));
    }

    fn step(&mut self, n: isize) {
        match self.focus {
            Focus::Files => self.move_selection(n as i32),
            Focus::Old => {
                self.old_cursor = clamp_line(self.old_cursor as isize + n, self.old_total());
                self.sync_from_old();
            }
            Focus::New => {
                self.new_cursor = clamp_line(self.new_cursor as isize + n, self.new_total());
                self.sync_from_new();
            }
        }
    }

    /// Jump to the top of the focused region (vim's `gg`).
    pub fn top(&mut self) {
        match self.focus {
            Focus::Files => {
                self.store_view();
                self.selected = 0;
                self.file_scroll = 0;
                self.load_diff();
            }
            Focus::Old => {
                self.old_cursor = 0;
                self.sync_from_old();
            }
            Focus::New => {
                self.new_cursor = 0;
                self.sync_from_new();
            }
        }
    }

    /// Jump to the bottom of the focused region (vim's `G`): the last line of
    /// the focused pane, or the last file when the file list is focused.
    pub fn bottom(&mut self) {
        match self.focus {
            Focus::Files => {
                self.store_view();
                self.selected = self.files.len().saturating_sub(1);
                self.ensure_file_visible();
                self.load_diff();
            }
            Focus::Old => {
                self.old_cursor = self.old_total().saturating_sub(1);
                self.sync_from_old();
            }
            Focus::New => {
                self.new_cursor = self.new_total().saturating_sub(1);
                self.sync_from_new();
            }
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = self.neighbour(true);
        self.remember_diff();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.neighbour(false);
        self.remember_diff();
    }

    /// Track the last diff pane focused, so hiding the file panel can return
    /// there instead of landing on nothing.
    fn remember_diff(&mut self) {
        if matches!(self.focus, Focus::Old | Focus::New) {
            self.last_diff = self.focus;
        }
    }

    /// The pane that zoom (Ctrl-z) is currently hiding, if any.
    fn hidden_pane(&self) -> Option<Focus> {
        match self.zoom {
            Some(Focus::Old) => Some(Focus::New),
            Some(Focus::New) => Some(Focus::Old),
            _ => None,
        }
    }

    /// Whether a region is currently on screen: the file panel unless toggled
    /// off, and neither a diff pane that zoom hides.
    fn visible(&self, f: Focus) -> bool {
        match f {
            Focus::Files => self.show_files,
            _ => Some(f) != self.hidden_pane(),
        }
    }

    /// The region one step left/right within the visible panels, skipping any
    /// region currently hidden (the file panel when toggled off, or a diff pane
    /// that zoom hides). Movement still stops at the ends (it never wraps), so
    /// when the only step lands on a hidden region with no visible panel past
    /// it, the focus stays put.
    ///
    /// Hiding is deliberately left untouched here: focus only moves between
    /// regions that are actually on screen, so the zoom target and panel state
    /// stay valid.
    fn neighbour(&self, right: bool) -> Focus {
        let step = |f: Focus| if right { f.right() } else { f.left() };
        let mut next = step(self.focus);
        while !self.visible(next) {
            let further = step(next);
            if further == next {
                return self.focus;
            }
            next = further;
        }
        next
    }

    /// Toggle the file (git) panel. Hiding it while focused returns focus to the
    /// last diff pane so movement keys keep working; showing it focuses it.
    pub fn toggle_files(&mut self) {
        if self.show_files {
            self.show_files = false;
            if self.focus == Focus::Files {
                self.focus = self.last_diff;
            }
        } else {
            self.show_files = true;
            self.focus = Focus::Files;
        }
    }

    /// Toggle single-pane zoom on the focused diff pane. No-op when the file
    /// list is focused — zoom only applies to the two code panes.
    pub fn toggle_zoom(&mut self) {
        let target = match self.focus {
            Focus::Old => Focus::Old,
            Focus::New => Focus::New,
            Focus::Files => return,
        };
        self.zoom = if self.zoom == Some(target) {
            None
        } else {
            Some(target)
        };
    }

    // ---- horizontal cursor (vim's w/b/e, 0/^/$) ----

    /// The text of `line` on a side (the code shown in that pane).
    pub fn side_text(&self, old_side: bool, line: usize) -> &str {
        let row = if old_side {
            model::old_row(&self.rows, line)
        } else {
            model::new_row(&self.rows, line)
        };
        match self.rows.get(row) {
            Some(DiffRow::Ctx(t)) | Some(DiffRow::Del(t)) | Some(DiffRow::Ins(t)) => t,
            Some(DiffRow::Chg(o, n)) => {
                if old_side {
                    o
                } else {
                    n
                }
            }
            None => "",
        }
    }

    /// The cursor column on a side, clamped to the line (an empty line is 0).
    pub fn cursor_col(&self, old_side: bool) -> usize {
        let (col, line) = if old_side {
            (self.old_col, self.old_cursor)
        } else {
            (self.new_col, self.new_cursor)
        };
        clamp_col(col, self.side_text(old_side, line).chars().count())
    }

    /// Horizontal scroll offset for a side.
    pub fn left(&self, old_side: bool) -> usize {
        if old_side {
            self.old_left
        } else {
            self.new_left
        }
    }

    /// The focused pane's line as chars plus the effective cursor column, or
    /// `None` when the file list is focused.
    fn focused_line(&self) -> Option<(Vec<char>, usize)> {
        let old_side = match self.focus {
            Focus::Old => true,
            Focus::New => false,
            Focus::Files => return None,
        };
        let text = self.side_text(
            old_side,
            if old_side {
                self.old_cursor
            } else {
                self.new_cursor
            },
        );
        let chars: Vec<char> = text.chars().collect();
        let col = clamp_col(
            if old_side { self.old_col } else { self.new_col },
            chars.len(),
        );
        Some((chars, col))
    }

    /// Move the focused pane's cursor column (no-op on the file list).
    fn set_col(&mut self, col: usize) {
        match self.focus {
            Focus::Old => self.old_col = col,
            Focus::New => self.new_col = col,
            Focus::Files => return,
        }
        self.keep_cols_visible();
    }

    /// Move the cursor one column, clamped to the line (vim's `h`/`l`).
    fn move_col(&mut self, delta: isize) {
        let Some((chars, col)) = self.focused_line() else {
            return;
        };
        if chars.is_empty() {
            return;
        }
        let last = chars.len() as isize - 1;
        self.set_col((col as isize + delta).clamp(0, last) as usize);
    }

    pub fn col_left(&mut self) {
        self.move_col(-1);
    }

    pub fn col_right(&mut self) {
        self.move_col(1);
    }

    pub fn line_start(&mut self) {
        self.set_col(0);
    }

    /// First non-blank column (vim's `^`).
    pub fn line_first_non_blank(&mut self) {
        if let Some((chars, _)) = self.focused_line() {
            let col = chars.iter().take_while(|c| c.is_whitespace()).count();
            self.set_col(col);
        }
    }

    /// Last character of the line (vim's `$`).
    pub fn line_end(&mut self) {
        if let Some((chars, _)) = self.focused_line() {
            self.set_col(chars.len().saturating_sub(1));
        }
    }

    /// Run a word motion over the focused side, crossing line boundaries like
    /// vim (line breaks act as non-word separators).
    fn word_move(&mut self, big: bool, f: fn(&[FlatChar], usize, bool) -> usize) {
        let old_side = match self.focus {
            Focus::Old => true,
            Focus::New => false,
            Focus::Files => return,
        };
        let (flat, cur) = self.side_flat(old_side);
        if flat.is_empty() {
            return;
        }
        let dest = f(&flat, cur, big);
        let (line, col) = (flat[dest].0, flat[dest].1);
        if old_side {
            self.old_cursor = line;
            self.old_col = col;
            self.sync_from_old();
        } else {
            self.new_cursor = line;
            self.new_col = col;
            self.sync_from_new();
        }
    }

    /// One side's text as a flat char sequence plus the cursor's index in it.
    /// A sentinel `'\n'` is appended after every line so word motions cannot
    /// run a word across a line break.
    fn side_flat(&self, old_side: bool) -> (Vec<FlatChar>, usize) {
        let (cur_line, cur_col) = if old_side {
            (self.old_cursor, self.old_col)
        } else {
            (self.new_cursor, self.new_col)
        };
        let mut flat: Vec<FlatChar> = Vec::new();
        let mut cursor = 0usize;
        let mut line = 0usize;
        for r in &self.rows {
            let Some(text) = row_text(r, old_side) else {
                continue;
            };
            let chars: Vec<char> = text.chars().collect();
            if line == cur_line {
                cursor = flat.len() + clamp_col(cur_col, chars.len());
            }
            for (col, ch) in chars.iter().enumerate() {
                flat.push((line, col, *ch));
            }
            flat.push((line, chars.len(), '\n'));
            line += 1;
        }
        (flat, cursor)
    }

    pub fn word_forward(&mut self) {
        self.word_move(false, flat_next_word);
    }

    pub fn word_back(&mut self) {
        self.word_move(false, flat_prev_word);
    }

    pub fn word_end(&mut self) {
        self.word_move(false, flat_word_end);
    }

    /// WORD variants (`W`/`B`/`E`): whitespace-delimited, punctuation included.
    pub fn word_forward_big(&mut self) {
        self.word_move(true, flat_next_word);
    }

    pub fn word_back_big(&mut self) {
        self.word_move(true, flat_prev_word);
    }

    pub fn word_end_big(&mut self) {
        self.word_move(true, flat_word_end);
    }

    /// Scroll each pane horizontally so its cursor column is on screen.
    pub fn keep_cols_visible(&mut self) {
        self.keep_side_visible(true);
        self.keep_side_visible(false);
    }

    fn keep_side_visible(&mut self, old_side: bool) {
        let (col, line, width) = if old_side {
            (self.old_col, self.old_cursor, self.old_code_w)
        } else {
            (self.new_col, self.new_cursor, self.new_code_w)
        };
        let eff = clamp_col(col, self.side_text(old_side, line).chars().count());
        let w = width.max(1);
        let mut left = if old_side {
            self.old_left
        } else {
            self.new_left
        };
        if eff < left {
            left = eff;
        } else if eff >= left + w {
            left = eff + 1 - w;
        }
        left = left.min(eff);
        if old_side {
            self.old_left = left;
        } else {
            self.new_left = left;
        }
    }

    // ---- `s` jump (flash.nvim-style) ----

    pub fn jump_state(&self) -> JumpState {
        self.jump
    }

    pub fn jump_active(&self) -> bool {
        self.jump != JumpState::Idle
    }

    /// The search pattern typed so far.
    pub fn jump_pattern(&self) -> &str {
        &self.jump_pattern
    }

    pub fn jump_matches(&self) -> &[JumpMatch] {
        &self.jump_matches
    }

    /// The pattern's length in characters (how much to tint after a match).
    pub fn jump_pattern_len(&self) -> usize {
        self.jump_pattern.chars().count()
    }

    /// Begin a jump. Needs a diff pane focused (the file list has no columns).
    pub fn start_jump(&mut self) {
        if matches!(self.focus, Focus::Files) {
            self.message = "s: focus a diff pane first".into();
            return;
        }
        self.jump = JumpState::Active;
        self.jump_pattern.clear();
        self.jump_matches.clear();
        self.message.clear();
    }

    /// Route a key to the jump mode. Returns true if it was consumed.
    ///
    /// Mirrors flash: every character either picks a label or extends the
    /// pattern, so you can keep typing to narrow the search.
    pub fn jump_key(&mut self, code: KeyCode) -> bool {
        if self.jump == JumpState::Idle {
            return false;
        }
        match code {
            KeyCode::Esc => self.cancel_jump(),
            KeyCode::Enter => self.jump_to_first(),
            KeyCode::Backspace => {
                self.jump_pattern.pop();
                self.refresh_jump();
            }
            KeyCode::Char(c) => self.jump_char(c),
            _ => self.cancel_jump(),
        }
        true
    }

    fn cancel_jump(&mut self) {
        self.jump = JumpState::Idle;
        self.jump_pattern.clear();
        self.jump_matches.clear();
    }

    fn jump_char(&mut self, c: char) {
        // A character is a label if one is currently assigned to it; labels are
        // never pattern continuations (see `jump_labels`), so this is unambiguous.
        if let Some(m) = self.jump_matches.iter().find(|m| m.label == Some(c)) {
            let m = m.clone();
            self.jump_to(&m);
            self.cancel_jump();
            return;
        }
        self.jump_pattern.push(c);
        self.refresh_jump();
    }

    fn jump_to_first(&mut self) {
        if let Some(m) = self.jump_matches.first().cloned() {
            self.jump_to(&m);
        }
        self.cancel_jump();
    }

    /// Recompute matches and labels for the current pattern.
    fn refresh_jump(&mut self) {
        self.jump_matches = self.visible_matches();
        if self.jump_matches.is_empty() {
            if !self.jump_pattern.is_empty() {
                self.message = format!("no match: {}", self.jump_pattern);
            }
            return;
        }
        let (line, col, width) = match self.focus {
            Focus::Old => (self.old_cursor, self.cursor_col(true), self.old_code_w),
            Focus::New => (self.new_cursor, self.cursor_col(false), self.new_code_w),
            Focus::Files => (0, 0, 1),
        };
        sort_by_distance(&mut self.jump_matches, line, col, width.max(1));
        let labels = self.jump_labels();
        assign_labels(&mut self.jump_matches, labels);
        self.message.clear();
    }

    /// Available labels for the current pattern: flash's alphabet minus any
    /// character that follows an occurrence of the pattern (those must stay
    /// typeable so the pattern can be extended).
    fn jump_labels(&self) -> Vec<char> {
        let mut labels = jump_label_alphabet();
        if self.jump_pattern.is_empty() {
            return labels;
        }
        let pattern: Vec<char> = self.jump_pattern.chars().collect();
        let old_side = match self.focus {
            Focus::Old => true,
            Focus::New => false,
            Focus::Files => return labels,
        };
        let (top, _, _) = if old_side {
            (self.old_top, self.old_left, self.old_code_w)
        } else {
            (self.new_top, self.new_left, self.new_code_w)
        };
        let bottom = top + self.viewport.max(1);
        let mut blocked: Vec<char> = Vec::new();
        for (row, r) in self.rows.iter().enumerate() {
            if row < top || row >= bottom {
                continue;
            }
            let Some(text) = row_text(r, old_side) else {
                continue;
            };
            let chars: Vec<char> = text.chars().collect();
            let mut i = 0;
            while i + pattern.len() <= chars.len() {
                if chars_match(&chars[i..i + pattern.len()], &pattern) {
                    if let Some(&next) = chars.get(i + pattern.len()) {
                        let lower = next.to_ascii_lowercase();
                        if !blocked.contains(&lower) {
                            blocked.push(lower);
                        }
                    }
                    i += pattern.len();
                } else {
                    i += 1;
                }
            }
        }
        labels.retain(|c| !blocked.contains(&c.to_ascii_lowercase()));
        labels
    }

    /// Every visible occurrence of the pattern (case-insensitive) in the focused
    /// pane's code window, in reading order.
    fn visible_matches(&self) -> Vec<JumpMatch> {
        let old_side = match self.focus {
            Focus::Old => true,
            Focus::New => false,
            Focus::Files => return Vec::new(),
        };
        let pattern: Vec<char> = self.jump_pattern.chars().collect();
        let plen = pattern.len();
        let (top, left, width) = if old_side {
            (self.old_top, self.old_left, self.old_code_w)
        } else {
            (self.new_top, self.new_left, self.new_code_w)
        };
        let bottom = top + self.viewport.max(1);
        let right = left + width;
        let mut out = Vec::new();
        if plen == 0 {
            return out;
        }
        let mut line = 0usize;
        for (row, r) in self.rows.iter().enumerate() {
            let Some(text) = row_text(r, old_side) else {
                continue;
            };
            if row >= top && row < bottom {
                let chars: Vec<char> = text.chars().collect();
                let mut i = 0;
                while i + plen <= chars.len() {
                    // Keep the whole match (and its trailing label) on screen.
                    if i >= left && i + plen < right && chars_match(&chars[i..i + plen], &pattern) {
                        out.push(JumpMatch {
                            old_side,
                            line,
                            col: i,
                            label: None,
                        });
                        i += plen;
                    } else {
                        i += 1;
                    }
                }
            }
            line += 1;
        }
        out
    }

    /// Move the cursor to a match and bring it into view (both axes).
    fn jump_to(&mut self, m: &JumpMatch) {
        let last = if m.old_side {
            self.old_total()
        } else {
            self.new_total()
        };
        let line = m.line.min(last.saturating_sub(1));
        if m.old_side {
            self.old_cursor = line;
            self.old_col = m.col;
            self.sync_from_old();
        } else {
            self.new_cursor = line;
            self.new_col = m.col;
            self.sync_from_new();
        }
    }

    // ---- `/` search (literal, smartcase, over the whole focused side) ----

    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    pub fn search_input(&self) -> bool {
        self.search_input
    }

    /// Open the search prompt on the focused pane.
    pub fn start_search(&mut self) {
        let old_side = match self.focus {
            Focus::Old => true,
            Focus::New => false,
            Focus::Files => {
                self.message = "/: focus a diff pane first".into();
                return;
            }
        };
        let (line, col) = if old_side {
            (self.old_cursor, self.cursor_col(true))
        } else {
            (self.new_cursor, self.cursor_col(false))
        };
        self.search_origin = (old_side, line, col);
        self.search = Some(Search {
            old_side,
            pattern: String::new(),
            matches: Vec::new(),
        });
        self.search_input = true;
        self.message.clear();
    }

    /// Route a key to the search prompt. Returns true if it was consumed.
    pub fn search_key(&mut self, code: KeyCode) -> bool {
        if !self.search_input {
            return false;
        }
        match code {
            KeyCode::Esc => self.cancel_search(),
            KeyCode::Enter => self.commit_search(),
            KeyCode::Backspace => {
                if let Some(s) = self.search.as_mut() {
                    s.pattern.pop();
                }
                self.refresh_search_preview();
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.search.as_mut() {
                    s.pattern.push(c);
                }
                self.refresh_search_preview();
            }
            _ => {}
        }
        true
    }

    fn cancel_search(&mut self) {
        self.search_input = false;
        self.search = None;
        self.restore_search_origin();
    }

    fn commit_search(&mut self) {
        self.search_input = false;
        let empty_or_no_match = match self.search.as_ref() {
            Some(s) => s.pattern.is_empty() || s.matches.is_empty(),
            None => true,
        };
        if empty_or_no_match {
            self.search = None;
            self.restore_search_origin();
            return;
        }
        // Land on the previewed match: the first one after the origin.
        let origin = (self.search_origin.1, self.search_origin.2);
        let hit = match self.search.as_ref() {
            Some(s) => first_after(&s.matches, origin)
                .or_else(|| s.matches.first())
                .copied(),
            None => None,
        };
        if let Some(hit) = hit {
            let side = self.search.as_ref().map(|s| s.old_side).unwrap_or(true);
            self.search_goto(side, hit);
        }
    }

    /// Recompute matches for the pattern and preview the first match from the
    /// origin (Vim's `incsearch`); with no match, sit back on the origin.
    fn refresh_search_preview(&mut self) {
        let Some(s) = self.search.as_ref() else {
            return;
        };
        let (side, pattern) = (s.old_side, s.pattern.clone());
        let matches = self.scan_search(side, &pattern);
        let preview = first_after(&matches, (self.search_origin.1, self.search_origin.2)).copied();
        if let Some(s) = self.search.as_mut() {
            s.matches = matches;
        }
        match preview {
            Some(hit) => self.search_goto(side, hit),
            None => self.restore_search_origin(),
        }
    }

    pub fn search_next(&mut self) {
        if let Some(hit) = self.search_first(true) {
            let side = self.search.as_ref().map(|s| s.old_side).unwrap_or(true);
            self.search_goto(side, hit);
        }
    }

    pub fn search_prev(&mut self) {
        if let Some(hit) = self.search_first(false) {
            let side = self.search.as_ref().map(|s| s.old_side).unwrap_or(true);
            self.search_goto(side, hit);
        }
    }

    /// The next/previous match relative to the search side's cursor (wrapping).
    fn search_first(&self, forward: bool) -> Option<SearchHit> {
        let s = self.search.as_ref()?;
        if s.matches.is_empty() {
            return None;
        }
        let pos = if s.old_side {
            (self.old_cursor, self.cursor_col(true))
        } else {
            (self.new_cursor, self.cursor_col(false))
        };
        let found = if forward {
            first_after(&s.matches, pos)
        } else {
            last_before(&s.matches, pos)
        };
        // Wrap around when there is nothing further in that direction.
        found.copied().or_else(|| {
            if forward {
                s.matches.first().copied()
            } else {
                s.matches.last().copied()
            }
        })
    }

    fn search_goto(&mut self, old_side: bool, hit: SearchHit) {
        if old_side {
            self.old_cursor = hit.line;
            self.old_col = hit.col;
            self.sync_from_old();
        } else {
            self.new_cursor = hit.line;
            self.new_col = hit.col;
            self.sync_from_new();
        }
    }

    fn restore_search_origin(&mut self) {
        let (side, line, col) = self.search_origin;
        if side {
            self.old_cursor = line;
            self.old_col = col;
            self.sync_from_old();
        } else {
            self.new_cursor = line;
            self.new_col = col;
            self.sync_from_new();
        }
    }

    /// All matches of `pattern` on a side, in reading order. Smartcase: any
    /// uppercase makes it case-sensitive, otherwise it is case-insensitive.
    fn scan_search(&self, old_side: bool, pattern: &str) -> Vec<SearchHit> {
        let pat: Vec<char> = pattern.chars().collect();
        if pat.is_empty() {
            return Vec::new();
        }
        let sensitive = pattern.chars().any(|c| c.is_uppercase());
        let mut out = Vec::new();
        let mut line = 0usize;
        for r in &self.rows {
            let Some(text) = row_text(r, old_side) else {
                continue;
            };
            let chars: Vec<char> = text.chars().collect();
            let mut i = 0;
            while i + pat.len() <= chars.len() {
                if chars_match_case(&chars[i..i + pat.len()], &pat, sensitive) {
                    out.push(SearchHit {
                        line,
                        col: i,
                        len: pat.len(),
                    });
                    i += pat.len();
                } else {
                    i += 1;
                }
            }
            line += 1;
        }
        out
    }

    // ---- cursor-line placement (vim's zz / zt / zb) ----

    /// Frame the focused pane's cursor on `want_row`, keeping the other pane
    /// paired. No-op on the file list, which has no cursor line.
    fn align_cursor(&mut self, want_row: usize) {
        match self.focus {
            Focus::Old => self.frame(true, self.old_cursor, want_row),
            Focus::New => self.frame(false, self.new_cursor, want_row),
            Focus::Files => {}
        }
    }

    pub fn cursor_center(&mut self) {
        self.align_cursor(self.viewport / 2);
    }

    pub fn cursor_top(&mut self) {
        self.align_cursor(0);
    }

    pub fn cursor_bottom(&mut self) {
        self.align_cursor(self.viewport.saturating_sub(1));
    }

    // ---- scroll the view without moving the cursor line (Ctrl-e / Ctrl-y) ----

    /// Scroll the focused pane's view one line down / up, keeping the cursor on
    /// its line; the cursor is pushed along only once it would leave the window.
    fn scroll_view(&mut self, down: bool) {
        let (old, cursor, top) = match self.focus {
            Focus::Old => (true, self.old_cursor, self.old_top),
            Focus::New => (false, self.new_cursor, self.new_top),
            Focus::Files => return,
        };
        let vp = self.viewport.max(1);
        let row = cursor.saturating_sub(top);
        let (want_row, delta) = scroll_step(row, vp, down);
        let line = (cursor as isize + delta).max(0) as usize;
        self.frame(old, line, want_row);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_view(true);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_view(false);
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.files.is_empty() {
            return;
        }
        self.store_view();
        let last = self.files.len() as i32 - 1;
        self.selected = (self.selected as i32 + delta).clamp(0, last) as usize;
        self.ensure_file_visible();
        self.load_diff();
    }

    fn ensure_file_visible(&mut self) {
        let vp = self.viewport.max(1);
        let row = self.file_row.get(self.selected).copied().unwrap_or(0);
        if row < self.file_scroll as usize {
            self.file_scroll = row as u16;
        } else if row >= self.file_scroll as usize + vp {
            self.file_scroll = (row + 1 - vp) as u16;
        }
    }

    // ---- hunks ----

    pub fn hunks(&self) -> Vec<model::Hunk> {
        model::hunks(&self.rows)
    }

    /// Row the cursor is on, for hunk navigation.
    fn current_row(&self) -> usize {
        model::old_row(&self.rows, self.old_cursor).min(model::new_row(&self.rows, self.new_cursor))
    }

    pub fn next_hunk(&mut self) {
        let cur = self.current_row();
        if let Some(h) = self.hunks().into_iter().find(|h| h.row > cur) {
            self.goto_hunk(&h);
        }
    }

    pub fn prev_hunk(&mut self) {
        let cur = self.current_row();
        if let Some(h) = self.hunks().into_iter().rev().find(|h| h.row < cur) {
            self.goto_hunk(&h);
        }
    }

    /// Move the cursor to a hunk, scrolling only as far as needed. A hunk
    /// already on screen just receives the cursor (and its paired line); an
    /// off-screen one is framed with context by [`App::frame_hunk`].
    fn goto_hunk(&mut self, h: &model::Hunk) {
        let new_side = matches!(self.rows.get(h.row), Some(DiffRow::Ins(_)));
        let line = if new_side { h.new } else { h.old };
        let (top, total) = if new_side {
            (self.new_top, self.new_total())
        } else {
            (self.old_top, self.old_total())
        };
        let vp = self.viewport.max(1);
        if line >= top && line < top + vp {
            if new_side {
                self.new_cursor = line.min(total.saturating_sub(1));
                self.sync_from_new();
            } else {
                self.old_cursor = line.min(total.saturating_sub(1));
                self.sync_from_old();
            }
        } else {
            self.frame_hunk(h);
        }
    }

    /// Put a hunk's first row near the top, anchoring the cursor on whichever
    /// side that row lives (an insertion starts on the new side).
    fn frame_hunk(&mut self, h: &model::Hunk) {
        let new_side = matches!(self.rows.get(h.row), Some(DiffRow::Ins(_)));
        let line = if new_side { h.new } else { h.old };
        self.frame(!new_side, line, HUNK_CONTEXT);
    }

    // ---- alignment ----

    /// Move the old cursor, then place the paired new line on the same screen
    /// row so the two panes read side by side.
    fn sync_from_old(&mut self) {
        let c = self.old_cursor.min(self.old_total().saturating_sub(1));
        self.old_cursor = c;
        let Some(&pair) = self.new_of_old.get(c) else {
            return;
        };
        let p = pair.min(self.new_total().saturating_sub(1));
        self.new_cursor = p;
        let row = c - follow_top(self.old_top, c, self.viewport, self.old_total());
        let (old_top, new_top) = plan(c, p, self.old_total(), self.new_total(), self.viewport, row);
        self.old_top = old_top;
        self.new_top = new_top;
        self.keep_cols_visible();
    }

    /// Dual of [`App::sync_from_old`] for the new side.
    fn sync_from_new(&mut self) {
        let c = self.new_cursor.min(self.new_total().saturating_sub(1));
        self.new_cursor = c;
        let Some(&pair) = self.old_of_new.get(c) else {
            return;
        };
        let p = pair.min(self.old_total().saturating_sub(1));
        self.old_cursor = p;
        let row = c - follow_top(self.new_top, c, self.viewport, self.new_total());
        let (new_top, old_top) = plan(c, p, self.new_total(), self.old_total(), self.viewport, row);
        self.new_top = new_top;
        self.old_top = old_top;
        self.keep_cols_visible();
    }

    fn load_diff(&mut self) {
        // A different diff invalidates the search's cached match positions.
        self.search = None;
        self.search_input = false;
        self.rows.clear();
        self.message.clear();
        self.old_top = 0;
        self.new_top = 0;
        self.old_cursor = 0;
        self.new_cursor = 0;
        let Some(path) = self.files.get(self.selected).map(|f| f.path.clone()) else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        let entry = &self.files[self.selected];
        match repo.before_after(entry) {
            Ok((old, new)) => {
                self.rows = model::diff_rows(&old, &new);
                let (new_of_old, old_of_new) = model::line_maps(&self.rows);
                self.new_of_old = new_of_old;
                self.old_of_new = old_of_new;
            }
            Err(e) => {
                self.new_of_old.clear();
                self.old_of_new.clear();
                self.message = format!("diff: {e}");
            }
        }
        self.place_view(&path);
    }

    /// Remember the current file's view position.
    fn store_view(&mut self) {
        if let Some(path) = self.files.get(self.selected).map(|f| f.path.clone()) {
            self.view_state.insert(
                path,
                ViewState {
                    old_cursor: self.old_cursor,
                    new_cursor: self.new_cursor,
                    old_top: self.old_top,
                    new_top: self.new_top,
                },
            );
        }
    }

    /// Restore a previously opened file's position, or jump to the first hunk
    /// for a file we haven't shown yet.
    fn place_view(&mut self, path: &str) {
        let saved = self.view_state.get(path).copied();
        let vp = self.viewport.max(1);
        match saved {
            Some(state) => {
                self.old_cursor = clamp_line(state.old_cursor as isize, self.old_total());
                self.new_cursor = clamp_line(state.new_cursor as isize, self.new_total());
                self.old_top = state.old_top.min(self.old_total().saturating_sub(vp));
                self.new_top = state.new_top.min(self.new_total().saturating_sub(vp));
            }
            None => self.jump_to_first_hunk(),
        }
        self.keep_cols_visible();
    }

    /// Put the first hunk a few lines down so there's some context above it.
    /// Unlike hunk *navigation*, this always frames, even if the hunk starts
    /// on screen.
    fn jump_to_first_hunk(&mut self) {
        if let Some(h) = self.hunks().into_iter().next() {
            self.frame_hunk(&h);
        } else {
            self.frame(true, 0, 0);
        }
    }

    /// Move the anchor cursor to `line` on the given side and frame it
    /// `want_row` lines below the top, so a jumped-to hunk fills the window
    /// instead of landing on the last visible row.
    fn frame(&mut self, old_anchor: bool, line: usize, want_row: usize) {
        let (t_line, o_line, t_total, o_total) = if old_anchor {
            let t = line.min(self.old_total().saturating_sub(1));
            let o = self
                .new_of_old
                .get(t)
                .copied()
                .unwrap_or(0)
                .min(self.new_total().saturating_sub(1));
            (t, o, self.old_total(), self.new_total())
        } else {
            let t = line.min(self.new_total().saturating_sub(1));
            let o = self
                .old_of_new
                .get(t)
                .copied()
                .unwrap_or(0)
                .min(self.old_total().saturating_sub(1));
            (t, o, self.new_total(), self.old_total())
        };
        let (t_top, o_top) = place_at(t_line, o_line, t_total, o_total, self.viewport, want_row);
        if old_anchor {
            self.old_cursor = t_line;
            self.new_cursor = o_line;
            self.old_top = t_top;
            self.new_top = o_top;
        } else {
            self.new_cursor = t_line;
            self.old_cursor = o_line;
            self.new_top = t_top;
            self.old_top = o_top;
        }
        self.keep_cols_visible();
    }
}

/// A flat `(line, col, char)` code position. Each line ends with a `'\n'`
/// sentinel so word motions cannot span a line break.
type FlatChar = (usize, usize, char);

fn clamp_line(v: isize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        v.clamp(0, total as isize - 1) as usize
    }
}

/// Clamp a goal column to a line of `len` chars (an empty line uses column 0).
fn clamp_col(col: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        col.min(len - 1)
    }
}

/// Vim's word char. `big` selects a WORD: any run of non-blank characters
/// (`W`/`B`/`E`), versus a word of letters, digits and underscore (`w`/`b`/`e`).
fn is_word_char(c: char, big: bool) -> bool {
    if big {
        !c.is_whitespace()
    } else {
        c.is_alphanumeric() || c == '_'
    }
}

/// Start of the next word/WORD after `start` (vim's `w`/`W`). Operates on a
/// flat list of `(line, col, char)` with a `'\n'` sentinel after each line, so
/// it crosses line boundaries.
fn flat_next_word(flat: &[FlatChar], start: usize, big: bool) -> usize {
    let n = flat.len();
    let mut i = start.min(n - 1);
    while i < n && is_word_char(flat[i].2, big) {
        i += 1;
    }
    while i < n && !is_word_char(flat[i].2, big) {
        i += 1;
    }
    if i >= n {
        return last_real(flat, start);
    }
    i
}

/// Start of the previous word/WORD before `start` (vim's `b`/`B`).
fn flat_prev_word(flat: &[FlatChar], start: usize, big: bool) -> usize {
    let n = flat.len();
    if start == 0 || n == 0 {
        return start.min(n.saturating_sub(1));
    }
    let mut i = start - 1;
    while i > 0 && !is_word_char(flat[i].2, big) {
        i -= 1;
    }
    if !is_word_char(flat[i].2, big) {
        return start; // nothing before: stay put
    }
    while i > 0 && is_word_char(flat[i - 1].2, big) {
        i -= 1;
    }
    i
}

/// End of the word/WORD at/after `start` (vim's `e`/`E`), always advancing off
/// a word end and crossing line boundaries.
fn flat_word_end(flat: &[FlatChar], start: usize, big: bool) -> usize {
    let n = flat.len();
    if n == 0 {
        return start;
    }
    let mut i = start.min(n - 1);
    if i + 1 >= n || !is_word_char(flat[i].2, big) || !is_word_char(flat[i + 1].2, big) {
        i += 1;
    }
    while i < n && !is_word_char(flat[i].2, big) {
        i += 1;
    }
    if i >= n {
        return last_real(flat, start);
    }
    while i + 1 < n && is_word_char(flat[i + 1].2, big) {
        i += 1;
    }
    i
}

/// The last index in `flat` that is a real character (not a line sentinel).
fn last_real(flat: &[FlatChar], fallback: usize) -> usize {
    flat.iter()
        .rposition(|(_, _, ch)| *ch != '\n')
        .unwrap_or(fallback)
}

/// The code text a row shows on a side, or `None` if the row is exclusive to
/// the other side.
fn row_text(row: &DiffRow, old_side: bool) -> Option<&str> {
    match row {
        DiffRow::Ctx(t) => Some(t),
        DiffRow::Chg(o, n) => Some(if old_side { o } else { n }),
        DiffRow::Del(t) => old_side.then_some(t),
        DiffRow::Ins(t) => (!old_side).then_some(t),
    }
}

/// flash.nvim's label alphabet (`Config.labels`): home-row first.
const JUMP_LABELS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// The full label list: the alphabet plus its uppercase, giving 52 single
/// characters (flash's `label.uppercase = true`).
fn jump_label_alphabet() -> Vec<char> {
    let mut out: Vec<char> = JUMP_LABELS.chars().collect();
    out.extend(JUMP_LABELS.to_uppercase().chars());
    out
}

/// Order matches by flash's distance metric: a linear position
/// (`line * width + col`) measured from the cursor, so nearer rows dominate and
/// nearer columns win ties.
fn sort_by_distance(matches: &mut [JumpMatch], line: usize, col: usize, width: usize) {
    let from = line * width + col;
    matches.sort_by_key(|m| {
        let pos = m.line * width + m.col;
        (pos.abs_diff(from), m.line, m.col)
    });
}

/// Label matches nearest-first with the available single characters; anything
/// past the labels stays unlabelled (flash does not use multi-character labels).
fn assign_labels(matches: &mut [JumpMatch], labels: Vec<char>) {
    for (i, m) in matches.iter_mut().enumerate() {
        m.label = labels.get(i).copied();
    }
}

/// Case-insensitive comparison of a text slice against a pattern.
fn chars_match(text: &[char], pattern: &[char]) -> bool {
    text.len() == pattern.len()
        && text
            .iter()
            .zip(pattern)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Comparison honouring smartcase: exact when `sensitive`, else case-insensitive.
fn chars_match_case(text: &[char], pattern: &[char], sensitive: bool) -> bool {
    if sensitive {
        text == pattern
    } else {
        chars_match(text, pattern)
    }
}

/// The first search hit strictly after `pos` (line, col).
fn first_after(hits: &[SearchHit], pos: (usize, usize)) -> Option<&SearchHit> {
    hits.iter().find(|h| (h.line, h.col) > pos)
}

/// The last search hit strictly before `pos` (line, col).
fn last_before(hits: &[SearchHit], pos: (usize, usize)) -> Option<&SearchHit> {
    hits.iter().rev().find(|h| (h.line, h.col) < pos)
}

/// One scroll-view step: the row to place the cursor on, and how many lines to
/// move the cursor itself (0 unless it would otherwise leave the window).
fn scroll_step(row: usize, viewport: usize, down: bool) -> (usize, isize) {
    let last = viewport.max(1) - 1;
    if down {
        if row == 0 {
            (0, 1)
        } else {
            (row - 1, 0)
        }
    } else if row >= last {
        (last, -1)
    } else {
        (row + 1, 0)
    }
}

fn follow_top(top: usize, cursor: usize, viewport: usize, total: usize) -> usize {
    let vp = viewport.max(1);
    let mut t = top;
    if cursor < t {
        t = cursor;
    } else if cursor >= t + vp {
        t = cursor + 1 - vp;
    }
    t.min(total.saturating_sub(vp))
}

/// Choose the shared screen row for both cursors, returning the two pane tops.
///
/// `c`/`t_total` is the anchor side and `p`/`o_total` the other; `want_row` is
/// the row the anchor pane would like its cursor on. The row is squeezed into
/// what both panes can actually scroll to, so paired lines sit side by side
/// wherever possible (added/removed lines pair with their insertion point).
fn plan(
    c: usize,
    p: usize,
    t_total: usize,
    o_total: usize,
    viewport: usize,
    want_row: usize,
) -> (usize, usize) {
    let vp = viewport.max(1);
    let t_max = t_total.saturating_sub(vp);
    let o_max = o_total.saturating_sub(vp);
    let lo = c.saturating_sub(t_max).max(p.saturating_sub(o_max));
    let hi = (vp - 1).min(c).min(p);
    let row = if lo <= hi {
        want_row.clamp(lo, hi)
    } else {
        want_row.min(vp - 1)
    };
    (
        c.saturating_sub(row).min(t_max),
        p.saturating_sub(row).min(o_max),
    )
}

/// Place the anchor cursor on `want_row` (clamped only to the anchor pane's own
/// scrolling range), then clamp the other pane independently. Used when jumping
/// to a hunk, where framing wins over exact alignment: a block at the very end
/// still opens with its first line near the top instead of being dragged to the
/// bottom by the paired line (which may have no room).
fn place_at(
    c: usize,
    p: usize,
    t_total: usize,
    o_total: usize,
    viewport: usize,
    want_row: usize,
) -> (usize, usize) {
    let vp = viewport.max(1);
    let t_max = t_total.saturating_sub(vp);
    let o_max = o_total.saturating_sub(vp);
    let row = want_row.min(vp - 1).min(c).max(c.saturating_sub(t_max));
    (c - row, p.saturating_sub(row).min(o_max))
}

/// Construct an app with just diff rows, for view rendering tests.
#[cfg(test)]
pub(crate) fn test_app(rows: Vec<DiffRow>, viewport: usize) -> App {
    let (new_of_old, old_of_new) = model::line_maps(&rows);
    App {
        repo: None,
        files: Vec::new(),
        tree: Vec::new(),
        file_row: Vec::new(),
        selected: 0,
        file_scroll: 0,
        rows,
        old_top: 0,
        new_top: 0,
        old_cursor: 0,
        new_cursor: 0,
        old_col: 0,
        new_col: 0,
        old_left: 0,
        new_left: 0,
        focus: Focus::Old,
        zoom: None,
        show_files: true,
        last_diff: Focus::Old,
        viewport,
        old_code_w: 1,
        new_code_w: 1,
        new_of_old,
        old_of_new,
        view_state: HashMap::new(),
        jump: JumpState::Idle,
        jump_pattern: String::new(),
        jump_matches: Vec::new(),
        search: None,
        search_input: false,
        search_origin: (true, 0, 0),
        message: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An [`App`] with the given diff rows and viewport, without a repository.
    fn app_with(rows: Vec<DiffRow>, viewport: usize) -> App {
        let (new_of_old, old_of_new) = model::line_maps(&rows);
        App {
            repo: None,
            files: Vec::new(),
            tree: Vec::new(),
            file_row: Vec::new(),
            selected: 0,
            file_scroll: 0,
            rows,
            old_top: 0,
            new_top: 0,
            old_cursor: 0,
            new_cursor: 0,
            old_col: 0,
            new_col: 0,
            old_left: 0,
            new_left: 0,
            focus: Focus::Old,
            zoom: None,
            show_files: true,
            last_diff: Focus::Old,
            viewport,
            old_code_w: 1,
            new_code_w: 1,
            new_of_old,
            old_of_new,
            view_state: HashMap::new(),
            jump: JumpState::Idle,
            jump_pattern: String::new(),
            jump_matches: Vec::new(),
            search: None,
            search_input: false,
            search_origin: (true, 0, 0),
            message: String::new(),
        }
    }

    /// Two changed lines split by context, so `]c`/`[c` have somewhere to go:
    /// old/new line 3 (`d`→`D`) and line 10 (`k`→`K`).
    fn two_hunks() -> Vec<DiffRow> {
        vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Ctx("b".into()),
            DiffRow::Ctx("c".into()),
            DiffRow::Chg("d".into(), "D".into()),
            DiffRow::Ctx("e".into()),
            DiffRow::Ctx("f".into()),
            DiffRow::Ctx("g".into()),
            DiffRow::Ctx("h".into()),
            DiffRow::Ctx("i".into()),
            DiffRow::Ctx("j".into()),
            DiffRow::Chg("k".into(), "K".into()),
            DiffRow::Ctx("l".into()),
            DiffRow::Ctx("m".into()),
            DiffRow::Ctx("n".into()),
            DiffRow::Ctx("o".into()),
            DiffRow::Ctx("p".into()),
        ]
    }

    #[test]
    fn context_lines_share_a_row() {
        let (t, o) = plan(5, 5, 100, 100, 10, 5);
        assert_eq!((t, o), (0, 0));
        assert_eq!(5 - t, 5 - o);
    }

    #[test]
    fn deletion_recovers_alignment() {
        // Three old lines removed: at the context line after them the old pane
        // must scroll down so it still lines up with the new side.
        let (t, o) = plan(4, 1, 100, 97, 10, 4);
        assert_eq!((t, o), (3, 0));
        assert_eq!(4 - t, 1 - o);
    }

    #[test]
    fn clamps_at_the_end() {
        let (t, o) = plan(99, 96, 100, 97, 10, 9);
        assert_eq!((t, o), (90, 87));
        assert_eq!(99 - t, 96 - o);
    }

    #[test]
    fn jumping_to_a_trailing_block_frames_it_at_the_top() {
        // The old file is far longer than the new one, so the paired line of the
        // first deleted line is the last new line and has no room at row 3.
        // Framing still wins: the anchor cursor stays at row 3.
        let (t, o) = place_at(87, 85, 144, 86, 38, 3);
        assert_eq!(87 - t, 3);
        assert_eq!(o, 86 - 38); // other pane pinned to its end
    }

    #[test]
    fn focus_stops_at_the_panes() {
        assert_eq!(Focus::Files.left(), Focus::Files);
        assert_eq!(Focus::Files.right(), Focus::Old);
        assert_eq!(Focus::Old.left(), Focus::Files);
        assert_eq!(Focus::Old.right(), Focus::New);
        assert_eq!(Focus::New.left(), Focus::Old);
        assert_eq!(Focus::New.right(), Focus::New);
    }

    #[test]
    fn zoom_keeps_focus_off_the_hidden_pane() {
        // Zoomed on the old pane, Ctrl-l would step onto the hidden new pane, so
        // focus stays put; Ctrl-h still reaches the (visible) file list. Moving
        // focus to the file list leaves zoom as is, and Ctrl-l returns to the
        // zoomed pane.
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::Old;
        app.zoom = Some(Focus::Old);
        app.focus_next();
        assert_eq!(app.focus, Focus::Old);
        assert_eq!(app.zoom, Some(Focus::Old));
        app.focus_prev();
        assert_eq!(app.focus, Focus::Files);
        assert_eq!(app.zoom, Some(Focus::Old));
        app.focus_next();
        assert_eq!(app.focus, Focus::Old);
        assert_eq!(app.zoom, Some(Focus::Old));
    }

    #[test]
    fn zoom_skips_the_hidden_pane_towards_the_files() {
        // Zoomed on the new pane, Ctrl-h steps over the hidden old pane to the
        // file list instead of focusing something invisible, keeping zoom; the
        // visible pane is reachable again with Ctrl-l.
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::New;
        app.zoom = Some(Focus::New);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Files);
        assert_eq!(app.zoom, Some(Focus::New));
        app.focus_next();
        assert_eq!(app.focus, Focus::New);
        assert_eq!(app.zoom, Some(Focus::New));
    }

    #[test]
    fn zoom_can_be_toggled_off_from_the_zoomed_pane() {
        // The file list has no zoom target, so leaving it for the zoomed pane is
        // how zoom is cleared.
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::Files;
        app.zoom = Some(Focus::Old);
        app.focus_next();
        assert_eq!(app.focus, Focus::Old);
        app.toggle_zoom();
        assert_eq!(app.zoom, None);
    }

    #[test]
    fn hiding_the_panel_returns_focus_to_the_last_diff_pane() {
        // Reach the file list from the old pane (the last diff pane seen), then
        // hide the panel: focus falls back to that pane, not to nothing.
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::New;
        app.focus_prev();
        assert_eq!(app.focus, Focus::Old);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Files);
        app.toggle_files();
        assert!(!app.show_files);
        assert_eq!(app.focus, Focus::Old);
    }

    #[test]
    fn showing_the_panel_focuses_it() {
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::Old;
        app.toggle_files();
        assert!(!app.show_files);
        app.toggle_files();
        assert!(app.show_files);
        assert_eq!(app.focus, Focus::Files);
    }

    #[test]
    fn focus_skips_a_hidden_file_panel() {
        let mut app = app_with(two_hunks(), 12);
        app.show_files = false;
        app.focus = Focus::Old;
        // Nothing visible to the left, so the step stops.
        app.focus_prev();
        assert_eq!(app.focus, Focus::Old);
        app.focus_next();
        assert_eq!(app.focus, Focus::New);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Old);
    }

    #[test]
    fn hidden_panel_and_zoom_compose() {
        // With the panel hidden and the old pane zoomed, neither the panel nor
        // the hidden new pane is reachable, so both steps stay put.
        let mut app = app_with(two_hunks(), 12);
        app.show_files = false;
        app.focus = Focus::Old;
        app.zoom = Some(Focus::Old);
        app.focus_next();
        assert_eq!(app.focus, Focus::Old);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Old);
    }

    #[test]
    fn top_and_bottom_move_each_pane_to_its_ends() {
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::Old;
        app.bottom();
        assert_eq!(app.old_cursor, app.old_total() - 1);
        assert_eq!(app.new_cursor, app.new_total() - 1);
        app.top();
        assert_eq!((app.old_cursor, app.new_cursor), (0, 0));

        app.focus = Focus::New;
        app.bottom();
        assert_eq!(app.new_cursor, app.new_total() - 1);
        app.top();
        assert_eq!(app.new_cursor, 0);
    }

    /// A flat char list like `App::side_flat`, for the word-motion tests.
    fn flat(lines: &[&str]) -> Vec<FlatChar> {
        let mut out = Vec::new();
        for (line, text) in lines.iter().enumerate() {
            for (col, ch) in text.chars().enumerate() {
                out.push((line, col, ch));
            }
            out.push((line, text.chars().count(), '\n'));
        }
        out
    }

    #[test]
    fn word_motions_walk_a_line() {
        let f = flat(&["foo bar_baz  qux"]);
        let at = |i: usize| (f[i].0, f[i].1);
        assert_eq!(at(flat_next_word(&f, 0, false)), (0, 4));
        assert_eq!(at(flat_next_word(&f, 4, false)), (0, 13));
        assert_eq!(at(flat_prev_word(&f, 4, false)), (0, 0));
        assert_eq!(at(flat_prev_word(&f, 13, false)), (0, 4));
        assert_eq!(at(flat_word_end(&f, 0, false)), (0, 2));
        assert_eq!(at(flat_word_end(&f, 2, false)), (0, 10));
    }

    #[test]
    fn capital_word_motions_are_whitespace_delimited() {
        // `w` stops on punctuation; `W` treats a whole non-blank run as one.
        let f = flat(&["foo(bar, baz)"]);
        let at = |i: usize| (f[i].0, f[i].1);
        assert_eq!(at(flat_next_word(&f, 0, false)), (0, 4)); // "bar"
        assert_eq!(at(flat_next_word(&f, 0, true)), (0, 9)); // "baz)"
        assert_eq!(at(flat_word_end(&f, 0, false)), (0, 2)); // end of "foo"
        assert_eq!(at(flat_word_end(&f, 0, true)), (0, 7)); // end of "foo(bar,"
        assert_eq!(at(flat_prev_word(&f, 9, true)), (0, 0));
    }

    #[test]
    fn word_motions_cross_lines() {
        let f = flat(&["foo", "bar baz", "qux"]);
        let at = |i: usize| (f[i].0, f[i].1);
        let find = |line: usize, col: usize| {
            f.iter()
                .position(|&(l, c, _)| l == line && c == col)
                .unwrap()
        };
        // `w` from "foo" reaches "bar" on the next line.
        assert_eq!(at(flat_next_word(&f, 2, false)), (1, 0));
        // `e` stops at the end of "foo", then at the end of "bar".
        let e = flat_word_end(&f, 0, false);
        assert_eq!(at(e), (0, 2));
        let e2 = flat_word_end(&f, e, false);
        assert_eq!(at(e2), (1, 2));
        assert_eq!(at(flat_word_end(&f, e2, false)), (1, 6)); // "baz"
                                                              // `b` from "baz" reaches "bar", then "foo" on the previous line.
        assert_eq!(at(flat_prev_word(&f, find(1, 4), false)), (1, 0));
        assert_eq!(at(flat_prev_word(&f, find(1, 0), false)), (0, 0));
    }

    #[test]
    fn search_is_smartcase_literal() {
        let mut app = app_with(vec![DiffRow::Ctx("let Let LET".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        let folded: Vec<usize> = app.scan_search(true, "let").iter().map(|h| h.col).collect();
        assert_eq!(folded, vec![0, 4, 8]); // lowercase -> case-insensitive
        let exact: Vec<usize> = app.scan_search(true, "Let").iter().map(|h| h.col).collect();
        assert_eq!(exact, vec![4]); // uppercase present -> case-sensitive
    }

    #[test]
    fn search_scans_the_whole_side_not_just_the_view() {
        let rows: Vec<DiffRow> = (0..10)
            .map(|i| DiffRow::Ctx(format!("line {i} let")))
            .collect();
        let mut app = app_with(rows, 3); // viewport only shows 3 rows
        app.focus = Focus::Old;
        app.old_code_w = 40;
        let hits = app.scan_search(true, "let");
        assert_eq!(hits.len(), 10);
        assert!(hits.iter().any(|h| h.line == 9));
    }

    #[test]
    fn search_previews_commits_and_repeats() {
        let rows: Vec<DiffRow> = (0..5).map(|_| DiffRow::Ctx("let x".into())).collect();
        let mut app = app_with(rows, 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.old_cursor = 0;
        app.old_col = 0;
        app.start_search();
        assert!(app.search_input());
        for c in "let".chars() {
            app.search_key(KeyCode::Char(c));
        }
        // Incremental preview: first match after the origin (line 0, col 0).
        assert_eq!(app.old_cursor, 1);
        assert_eq!(app.cursor_col(true), 0);

        app.search_key(KeyCode::Enter);
        assert!(!app.search_input());
        assert_eq!(app.search().unwrap().matches.len(), 5);
        // `n` / `N` step and wrap.
        app.search_next();
        assert_eq!(app.old_cursor, 2);
        app.search_prev();
        assert_eq!(app.old_cursor, 1);
        app.search_prev();
        assert_eq!(app.old_cursor, 0);
        app.search_prev(); // wraps to the last match
        assert_eq!(app.old_cursor, 4);
        app.search_next(); // wraps back to the first
        assert_eq!(app.old_cursor, 0);
    }

    #[test]
    fn search_escape_restores_the_origin() {
        let rows: Vec<DiffRow> = (0..3).map(|_| DiffRow::Ctx("let".into())).collect();
        let mut app = app_with(rows, 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.old_cursor = 1;
        app.old_col = 0;
        app.start_search();
        for c in "let".chars() {
            app.search_key(KeyCode::Char(c));
        }
        assert_ne!(app.old_cursor, 1);
        app.search_key(KeyCode::Esc);
        assert!(!app.search_input());
        assert!(app.search().is_none());
        assert_eq!(app.old_cursor, 1); // back where we started
    }

    #[test]
    fn search_with_no_match_reports_and_restores() {
        let mut app = app_with(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_search();
        for c in "zzz".chars() {
            app.search_key(KeyCode::Char(c));
        }
        assert_eq!(app.old_cursor, 0); // no preview move
        app.search_key(KeyCode::Enter);
        assert!(!app.search_input());
        assert!(app.search().is_none());
    }

    #[test]
    fn search_needs_a_diff_pane() {
        let mut app = app_with(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Files;
        app.start_search();
        assert!(!app.search_input());
        assert!(app.message.contains("focus a diff pane"));
    }

    #[test]
    fn loading_a_diff_clears_the_search() {
        // Switching files (or reloading) must drop stale match positions.
        let mut app = app_with(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_search();
        app.search_key(KeyCode::Char('a'));
        app.search_key(KeyCode::Enter);
        assert!(app.search().is_some());
        app.load_diff();
        assert!(app.search().is_none());
        assert!(!app.search_input());
    }

    #[test]
    fn jump_labels_follow_flash() {
        // Home-row-first alphabet, then uppercase: 52 single-character labels.
        let labels = jump_label_alphabet();
        assert_eq!(labels.len(), 52);
        assert_eq!(labels[0], 'a');
        assert_eq!(labels[1], 's');
        assert_eq!(labels[25], 'm');
        assert_eq!(labels[26], 'A');

        // Matches past the alphabet stay unlabelled.
        let mut matches: Vec<JumpMatch> = (0..54)
            .map(|i| JumpMatch {
                old_side: true,
                line: 0,
                col: i,
                label: None,
            })
            .collect();
        assign_labels(&mut matches, jump_label_alphabet());
        assert_eq!(matches[0].label, Some('a'));
        assert_eq!(matches[51].label, Some('M'));
        assert_eq!(matches[52].label, None);
    }

    #[test]
    fn jump_labels_skip_pattern_continuations() {
        // With the pattern `l`, `e` follows every `l` in "let", so flash drops
        // it from the labels — otherwise you could not type "le".
        let mut app = app_with(vec![DiffRow::Ctx("let let".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_jump();
        app.jump_key(KeyCode::Char('l'));
        let labels = app.jump_labels();
        assert!(!labels.contains(&'e'), "e must stay typeable");
        assert!(!labels.contains(&'E'));
        assert!(labels.contains(&'a'));
        // `l` itself is also dropped (the second 'l'... actually 'e' only).
        assert!(labels.len() >= 50);
    }

    #[test]
    fn jump_extends_the_pattern_and_moves() {
        let mut app = app_with(vec![DiffRow::Ctx("let x let y let".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_jump();
        app.jump_key(KeyCode::Char('l'));
        assert_eq!(app.jump_pattern(), "l");
        // `e` is not a label (it follows `l`), so it extends the pattern.
        app.jump_key(KeyCode::Char('e'));
        assert_eq!(app.jump_pattern(), "le");
        app.jump_key(KeyCode::Char('t'));
        assert_eq!(app.jump_pattern(), "let");
        assert_eq!(app.jump_matches().len(), 3);
        // The three "let" matches, nearest-first from col 0.
        let labels: Vec<Option<char>> = app.jump_matches().iter().map(|m| m.label).collect();
        assert_eq!(labels, vec![Some('a'), Some('s'), Some('d')]);
        // The second label jumps to the second "let" (col 6).
        app.jump_key(KeyCode::Char('s'));
        assert_eq!(app.jump_state(), JumpState::Idle);
        assert_eq!(app.cursor_col(true), 6);
    }

    #[test]
    fn jump_single_match_waits_for_label_or_enter() {
        // flash's `autojump` is off: one match is labelled and waits.
        let mut app = app_with(vec![DiffRow::Ctx("abcabc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_jump();
        app.jump_key(KeyCode::Char('b'));
        assert_eq!(app.jump_matches().len(), 2);
        assert_eq!(app.jump_matches()[0].label, Some('a'));
        app.jump_key(KeyCode::Enter);
        assert_eq!(app.jump_state(), JumpState::Idle);
        assert_eq!(app.cursor_col(true), 1);
    }

    #[test]
    fn jump_reports_no_match_but_stays_active() {
        let mut app = app_with(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_jump();
        app.jump_key(KeyCode::Char('z'));
        assert_eq!(app.jump_state(), JumpState::Active);
        assert!(app.message.contains("no match"), "msg: {}", app.message);
        // Backspace clears the pattern again (flash's `<BS>`).
        app.jump_key(KeyCode::Backspace);
        assert_eq!(app.jump_pattern(), "");
        assert!(app.jump_matches().is_empty());
    }

    #[test]
    fn jump_escape_cancels() {
        let mut app = app_with(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_jump();
        assert!(app.jump_key(KeyCode::Esc));
        assert_eq!(app.jump_state(), JumpState::Idle);
    }

    #[test]
    fn jump_needs_a_diff_pane() {
        let mut app = app_with(vec![DiffRow::Ctx("aa".into())], 5);
        app.focus = Focus::Files;
        app.start_jump();
        assert_eq!(app.jump_state(), JumpState::Idle);
        assert!(app.message.contains("focus a diff pane"));
    }

    #[test]
    fn line_and_word_motions_move_the_column() {
        let mut app = app_with(vec![DiffRow::Ctx("foo bar".into())], 10);
        app.focus = Focus::Old;
        app.old_code_w = 8;
        app.line_end();
        assert_eq!(app.cursor_col(true), 6);
        app.line_start();
        assert_eq!(app.cursor_col(true), 0);
        app.word_forward();
        assert_eq!(app.cursor_col(true), 4);
        app.word_end();
        assert_eq!(app.cursor_col(true), 6);
        app.word_back();
        assert_eq!(app.cursor_col(true), 4);
    }

    #[test]
    fn caret_goes_to_the_first_non_blank() {
        let mut app = app_with(vec![DiffRow::Ctx("    x".into())], 10);
        app.focus = Focus::Old;
        app.old_code_w = 8;
        app.line_first_non_blank();
        assert_eq!(app.cursor_col(true), 4);
    }

    #[test]
    fn hjkl_move_by_line_and_column() {
        let mut app = app_with(
            vec![DiffRow::Ctx("abc".into()), DiffRow::Ctx("de".into())],
            10,
        );
        app.focus = Focus::Old;
        app.old_code_w = 8;
        app.col_right();
        app.col_right();
        assert_eq!(app.cursor_col(true), 2);
        // Clamped at the line end.
        app.col_right();
        assert_eq!(app.cursor_col(true), 2);
        app.col_left();
        assert_eq!(app.cursor_col(true), 1);
        // `j` keeps the goal column, clamped to the shorter line.
        app.down();
        assert_eq!(app.old_cursor, 1);
        assert_eq!(app.cursor_col(true), 1);
        app.col_right();
        assert_eq!(app.cursor_col(true), 1);
    }

    #[test]
    fn horizontal_scroll_follows_the_cursor_column() {
        let mut app = app_with(vec![DiffRow::Ctx("0123456789".into())], 10);
        app.focus = Focus::Old;
        app.old_code_w = 4;
        app.line_end();
        // A four-column window on column 9 starts at 6.
        assert_eq!(app.left(true), 6);
        app.line_start();
        assert_eq!(app.left(true), 0);
    }

    #[test]
    fn focus_moves_normally_without_zoom() {
        let mut app = app_with(two_hunks(), 12);
        app.focus = Focus::Old;
        app.focus_next();
        assert_eq!(app.focus, Focus::New);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Old);
    }

    #[test]
    fn scroll_view_moves_the_view_until_the_cursor_hits_an_edge() {
        // Away from the edges the cursor keeps its line and only the view moves.
        assert_eq!(scroll_step(5, 10, true), (4, 0));
        assert_eq!(scroll_step(5, 10, false), (6, 0));
        // At the top, scrolling down pushes the cursor down one line.
        assert_eq!(scroll_step(0, 10, true), (0, 1));
        // At the bottom, scrolling up pushes the cursor up one line.
        assert_eq!(scroll_step(9, 10, false), (9, -1));
    }

    #[test]
    fn next_hunk_keeps_the_view_when_the_hunk_is_already_visible() {
        let mut app = app_with(two_hunks(), 12);
        app.old_cursor = 3;
        app.new_cursor = 3;
        app.next_hunk();
        // Both hunks fit on screen, so only the cursor moves — no scroll.
        assert_eq!((app.old_top, app.new_top), (0, 0));
        assert_eq!((app.old_cursor, app.new_cursor), (10, 10));
    }

    #[test]
    fn next_hunk_frames_an_off_screen_hunk_with_context() {
        let mut app = app_with(two_hunks(), 6);
        app.old_cursor = 3;
        app.new_cursor = 3;
        app.next_hunk();
        // The second hunk is below the six-line window, so it is framed with
        // HUNK_CONTEXT lines above it.
        assert_eq!(app.old_cursor, 10);
        assert_eq!(app.old_top, 10 - HUNK_CONTEXT);
    }

    #[test]
    fn prev_hunk_keeps_the_view_when_the_hunk_is_already_visible() {
        let mut app = app_with(two_hunks(), 12);
        app.old_cursor = 10;
        app.new_cursor = 10;
        app.prev_hunk();
        assert_eq!((app.old_top, app.new_top), (0, 0));
        assert_eq!((app.old_cursor, app.new_cursor), (3, 3));
    }

    #[test]
    fn opening_a_file_still_frames_the_first_hunk() {
        // A first hunk that starts on screen is still framed three lines down,
        // so a freshly opened file opens with context above the change.
        let mut rows: Vec<DiffRow> = (0..8).map(|_| DiffRow::Ctx("ctx".into())).collect();
        rows.push(DiffRow::Chg("old".into(), "new".into()));
        rows.extend((0..21).map(|_| DiffRow::Ctx("ctx".into())));
        let mut app = app_with(rows, 12);
        app.jump_to_first_hunk();
        assert_eq!(app.old_cursor, 8);
        assert_eq!(app.old_top, 8 - HUNK_CONTEXT);
    }
}
