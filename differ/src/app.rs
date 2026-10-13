//! Application state, independent of rendering.

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

    fn sync_from_old(&mut self) {
        if let Some(&nc) = self.new_of_old.get(self.old_cursor) {
            self.new_cursor = nc.min(self.new_total().saturating_sub(1));
        }
        self.follow();
    }

    fn sync_from_new(&mut self) {
        if let Some(&oc) = self.old_of_new.get(self.new_cursor) {
            self.old_cursor = oc.min(self.old_total().saturating_sub(1));
        }
        self.follow();
    }

    /// Scroll each pane so its cursor stays on screen.
    fn follow(&mut self) {
        self.old_top = follow_top(
            self.old_top,
            self.old_cursor,
            self.viewport,
            self.old_total(),
        );
        self.new_top = follow_top(
            self.new_top,
            self.new_cursor,
            self.viewport,
            self.new_total(),
        );
    }

    fn load_diff(&mut self) {
        self.rows.clear();
        self.message.clear();
        self.old_top = 0;
        self.new_top = 0;
        self.old_cursor = 0;
        self.new_cursor = 0;
        let Some(repo) = &self.repo else { return };
        let Some(entry) = self.files.get(self.selected) else {
            return;
        };
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
