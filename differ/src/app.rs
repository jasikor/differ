//! Application state, independent of rendering.

use std::collections::HashMap;

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

    /// Run a word-motion over the focused line (no-op on the file list).
    fn word_move(&mut self, big: bool, f: fn(&[char], usize, bool) -> usize) {
        if let Some((chars, col)) = self.focused_line() {
            self.set_col(f(&chars, col, big));
        }
    }

    pub fn word_forward(&mut self) {
        self.word_move(false, next_word_start);
    }

    pub fn word_back(&mut self) {
        self.word_move(false, prev_word_start);
    }

    pub fn word_end(&mut self) {
        self.word_move(false, word_end_col);
    }

    /// WORD variants (`W`/`B`/`E`): whitespace-delimited, punctuation included.
    pub fn word_forward_big(&mut self) {
        self.word_move(true, next_word_start);
    }

    pub fn word_back_big(&mut self) {
        self.word_move(true, prev_word_start);
    }

    pub fn word_end_big(&mut self) {
        self.word_move(true, word_end_col);
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

/// Start of the next word/WORD at/after `col` (vim's `w`/`W`), clamped to the
/// line.
fn next_word_start(chars: &[char], col: usize, big: bool) -> usize {
    let n = chars.len();
    if n == 0 {
        return 0;
    }
    let mut i = col.min(n - 1);
    while i < n && is_word_char(chars[i], big) {
        i += 1;
    }
    while i < n && !is_word_char(chars[i], big) {
        i += 1;
    }
    i.min(n - 1)
}

/// Start of the previous word/WORD before `col` (vim's `b`/`B`).
fn prev_word_start(chars: &[char], col: usize, big: bool) -> usize {
    let n = chars.len();
    if n == 0 || col == 0 {
        return 0;
    }
    let mut i = col.min(n - 1) - 1;
    while i > 0 && !is_word_char(chars[i], big) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1], big) {
        i -= 1;
    }
    i
}

/// End of the word/WORD at/after `col` (vim's `e`/`E`), always advancing off a
/// word end.
fn word_end_col(chars: &[char], col: usize, big: bool) -> usize {
    let n = chars.len();
    if n == 0 {
        return 0;
    }
    let mut i = col.min(n - 1);
    if i + 1 >= n || !is_word_char(chars[i], big) || !is_word_char(chars[i + 1], big) {
        i += 1;
    }
    while i < n && !is_word_char(chars[i], big) {
        i += 1;
    }
    if i >= n {
        return n - 1;
    }
    while i + 1 < n && is_word_char(chars[i + 1], big) {
        i += 1;
    }
    i
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

    #[test]
    fn word_motions_walk_a_line() {
        let chars: Vec<char> = "foo bar_baz  qux".chars().collect();
        assert_eq!(next_word_start(&chars, 0, false), 4);
        assert_eq!(next_word_start(&chars, 4, false), 13);
        assert_eq!(prev_word_start(&chars, 4, false), 0);
        assert_eq!(prev_word_start(&chars, 13, false), 4);
        assert_eq!(word_end_col(&chars, 0, false), 2);
        assert_eq!(word_end_col(&chars, 2, false), 10);
    }

    #[test]
    fn capital_word_motions_are_whitespace_delimited() {
        // `w` stops on punctuation; `W` treats a whole non-blank run as one.
        let chars: Vec<char> = "foo(bar, baz)".chars().collect();
        assert_eq!(next_word_start(&chars, 0, false), 4); // "bar"
        assert_eq!(next_word_start(&chars, 0, true), 9); // "baz)"
        assert_eq!(word_end_col(&chars, 0, false), 2); // end of "foo"
        assert_eq!(word_end_col(&chars, 0, true), 7); // end of "foo(bar,"
        assert_eq!(prev_word_start(&chars, 9, true), 0);
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
