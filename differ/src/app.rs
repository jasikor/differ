//! Application state, independent of rendering.

use crate::git::{self, Repo};
use crate::model::{self, DiffRow};
use crate::view::theme::ViewOpts;

pub struct App {
    pub repo: Option<Repo>,
    pub files: Vec<git::FileEntry>,
    pub selected: usize,
    pub file_scroll: u16,
    pub rows: Vec<DiffRow>,
    pub scroll: u16,
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
            scroll: 0,
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

    /// Move the selection by `delta` files.
    pub fn move_selection(&mut self, delta: i32) {
        if self.files.is_empty() {
            return;
        }
        let last = self.files.len() as i32 - 1;
        self.selected = (self.selected as i32 + delta).clamp(0, last) as usize;
        self.scroll = 0;
        self.load_diff();
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.files.get(self.selected).map(|f| f.path.as_str())
    }

    /// Old-view row offsets where each change hunk begins.
    pub fn hunk_offsets(&self) -> Vec<usize> {
        model::hunk_offsets(&self.rows)
    }

    pub fn next_hunk(&mut self) {
        let starts = self.hunk_offsets();
        let cur = self.scroll as usize;
        if let Some(&s) = starts.iter().find(|&&s| s > cur) {
            self.scroll = s as u16;
        }
    }

    pub fn prev_hunk(&mut self) {
        let starts = self.hunk_offsets();
        let cur = self.scroll as usize;
        if let Some(&s) = starts.iter().rev().find(|&&s| s < cur) {
            self.scroll = s as u16;
        }
    }

    fn load_diff(&mut self) {
        self.rows.clear();
        self.message.clear();
        let Some(repo) = &self.repo else { return };
        let Some(entry) = self.files.get(self.selected) else {
            return;
        };
        match repo.before_after(entry) {
            Ok((old, new)) => self.rows = model::diff_rows(&old, &new),
            Err(e) => self.message = format!("diff: {e}"),
        }
    }
}
