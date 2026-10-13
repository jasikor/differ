//! Application state, independent of rendering.

use crate::git::{self, Repo};

pub struct App {
    pub repo: Option<Repo>,
    pub files: Vec<git::FileEntry>,
    pub message: String,
}

impl App {
    pub fn new() -> Self {
        match Repo::discover() {
            Ok(repo) => match repo.status() {
                Ok(files) => {
                    let message = format!("{} changed file(s)", files.len());
                    App {
                        repo: Some(repo),
                        files,
                        message,
                    }
                }
                Err(e) => App {
                    repo: Some(repo),
                    files: Vec::new(),
                    message: format!("git status failed: {e}"),
                },
            },
            Err(e) => App {
                repo: None,
                files: Vec::new(),
                message: format!("not a git repository: {e}"),
            },
        }
    }
}
