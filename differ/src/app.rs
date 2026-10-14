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
    pub focus: Focus,
    /// When set, that diff pane fills the width and the other pane, gutter and
    /// its ruler are hidden.
    pub zoom: Option<Focus>,
    /// Pane body height, refreshed by the view each frame.
    pub viewport: usize,
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
            focus: Focus::Files,
            zoom: None,
            viewport: 1,
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

    pub fn top(&mut self) {
        match self.focus {
            Focus::Files => {
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

    pub fn focus_next(&mut self) {
        self.focus = self.focus.right();
        self.follow_zoom();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.focus.left();
        self.follow_zoom();
    }

    /// While zoomed, the shown pane follows focus; focusing the file list (which
    /// has no zoom) leaves zoom.
    fn follow_zoom(&mut self) {
        if self.zoom.is_some() {
            self.zoom = match self.focus {
                Focus::Old => Some(Focus::Old),
                Focus::New => Some(Focus::New),
                Focus::Files => None,
            };
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

    /// Put a hunk's first row near the top, anchoring the cursor on whichever
    /// side that row lives (an insertion starts on the new side).
    fn goto_hunk(&mut self, h: &model::Hunk) {
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
    }

    /// Put the first hunk a few lines down so there's some context above it.
    fn jump_to_first_hunk(&mut self) {
        if let Some(h) = self.hunks().into_iter().next() {
            self.goto_hunk(&h);
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
    }
}

fn clamp_line(v: isize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        v.clamp(0, total as isize - 1) as usize
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn scroll_view_moves_the_view_until_the_cursor_hits_an_edge() {
        // Away from the edges the cursor keeps its line and only the view moves.
        assert_eq!(scroll_step(5, 10, true), (4, 0));
        assert_eq!(scroll_step(5, 10, false), (6, 0));
        // At the top, scrolling down pushes the cursor down one line.
        assert_eq!(scroll_step(0, 10, true), (0, 1));
        // At the bottom, scrolling up pushes the cursor up one line.
        assert_eq!(scroll_step(9, 10, false), (9, -1));
    }
}
