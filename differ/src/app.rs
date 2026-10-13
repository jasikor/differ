//! Application state, independent of rendering.

use std::collections::HashMap;

use crate::git::{self, Repo};
use crate::model::{self, DiffRow};
use crate::view::theme::ViewOpts;

/// Which region receives movement keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Old,
    New,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Files => Focus::Old,
            Focus::Old => Focus::New,
            Focus::New => Focus::Files,
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
    /// Pane body height, refreshed by the view each frame.
    pub viewport: usize,
    new_of_old: Vec<usize>,
    old_of_new: Vec<usize>,
    /// Saved view position per file path.
    view_state: HashMap<String, ViewState>,
    pub opts: ViewOpts,
    pub message: String,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            repo: None,
            files: Vec::new(),
            selected: 0,
            file_scroll: 0,
            rows: Vec::new(),
            old_top: 0,
            new_top: 0,
            old_cursor: 0,
            new_cursor: 0,
            focus: Focus::Files,
            viewport: 1,
            new_of_old: Vec::new(),
            old_of_new: Vec::new(),
            view_state: HashMap::new(),
            opts: ViewOpts::default(),
            message: String::new(),
        };
        match Repo::discover() {
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
            Ok(files) => {
                self.files = files;
                if self.selected >= self.files.len() {
                    self.selected = self.files.len().saturating_sub(1);
                }
            }
            Err(e) => self.message = format!("status: {e}"),
        }
        self.load_diff();
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

    pub fn toggle_focus(&mut self) {
        self.focus = self.focus.next();
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
        if self.selected < self.file_scroll as usize {
            self.file_scroll = self.selected as u16;
        } else if self.selected >= self.file_scroll as usize + vp {
            self.file_scroll = (self.selected + 1 - vp) as u16;
        }
    }

    // ---- hunks ----

    pub fn hunk_offsets(&self) -> Vec<usize> {
        model::hunk_offsets(&self.rows)
    }

    pub fn next_hunk(&mut self) {
        let cur = self.old_cursor;
        if let Some(&s) = self.hunk_offsets().iter().find(|&&s| s > cur) {
            self.old_cursor = s.min(self.old_total().saturating_sub(1));
            self.sync_from_old();
        }
    }

    pub fn prev_hunk(&mut self) {
        let cur = self.old_cursor;
        if let Some(&s) = self.hunk_offsets().iter().rev().find(|&&s| s < cur) {
            self.old_cursor = s.min(self.old_total().saturating_sub(1));
            self.sync_from_old();
        }
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
        let (c, want) = match self.hunk_offsets().first().copied() {
            Some(h) => (h.min(self.old_total().saturating_sub(1)), 3),
            None => (0, 0),
        };
        self.old_cursor = c;
        let p = self
            .new_of_old
            .get(c)
            .copied()
            .unwrap_or(0)
            .min(self.new_total().saturating_sub(1));
        self.new_cursor = p;
        let vp = self.viewport.max(1);
        let (old_top, new_top) = plan(
            c,
            p,
            self.old_total(),
            self.new_total(),
            vp,
            want.min(vp - 1),
        );
        self.old_top = old_top;
        self.new_top = new_top;
    }
}

fn clamp_line(v: isize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        v.clamp(0, total as isize - 1) as usize
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
}
