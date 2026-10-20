//! Thin wrapper around the system `git`, driven through `std::process::Command`.
//!
//! Everything shells out to `git` so the tool inherits the user's real
//! configuration (remotes, ssh, credential helpers, hooks, signing).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// `git` exited non-zero.
    Git {
        args: Vec<String>,
        code: Option<i32>,
        stderr: String,
    },
    NonUtf8,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Git { args, code, stderr } => {
                let stderr = stderr.trim();
                write!(f, "git {} failed", args.join(" "))?;
                if let Some(code) = code {
                    write!(f, " (exit {code})")?;
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Error::NonUtf8 => write!(f, "git produced non-UTF-8 output"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Run `git <args>` (optionally in `dir`) and return its stdout.
fn run(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let out = cmd.args(args).output()?;
    if !out.status.success() {
        return Err(Error::Git {
            args: args.iter().map(|s| (*s).to_string()).collect(),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    String::from_utf8(out.stdout).map_err(|_| Error::NonUtf8)
}

/// A single changed path, closest to `git status`'s two-column XY state.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    /// Staged (index) change, i.e. the `X` column.
    pub staged: Option<char>,
    /// Unstaged (worktree) change, i.e. the `Y` column.
    pub unstaged: Option<char>,
}

/// A discovered git repository. Commands run with its root as the working
/// directory so paths are always repo-relative.
#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// Discover the repo containing `start`, or the current directory.
    pub fn discover_in(start: Option<&Path>) -> Result<Self> {
        let out = run(start, &["rev-parse", "--show-toplevel"])?;
        Ok(Repo {
            root: PathBuf::from(out.trim_end()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        run(Some(&self.root), args)
    }

    /// Changed paths via `git status --porcelain=v2`.
    pub fn status(&self) -> Result<Vec<FileEntry>> {
        let out = self.git(&["status", "--porcelain=v2", "--untracked-files=all"])?;
        Ok(parse_status(&out))
    }

    /// `git show <spec>` (e.g. `:path` for the index, `HEAD:path` for HEAD).
    pub fn show(&self, spec: &str) -> Result<String> {
        self.git(&["show", spec])
    }

    pub fn worktree(&self, path: &str) -> Result<String> {
        std::fs::read_to_string(self.root.join(path)).map_err(Error::Io)
    }

    /// Before/after text for a changed file, mirroring `git status`:
    /// unstaged = index ↔ worktree, staged = HEAD ↔ index. Added/untracked have
    /// an empty before; deleted has an empty after.
    pub fn before_after(&self, entry: &FileEntry) -> Result<(String, String)> {
        let path = &entry.path;
        if let Some(y) = entry.unstaged {
            let before = if y == '?' {
                String::new()
            } else {
                self.show(&format!(":{path}"))?
            };
            let after = if y == 'D' {
                String::new()
            } else {
                self.worktree(path)?
            };
            Ok((before, after))
        } else if let Some(x) = entry.staged {
            let before = if x == 'A' {
                String::new()
            } else {
                self.show(&format!("HEAD:{path}"))?
            };
            let after = if x == 'D' {
                String::new()
            } else {
                self.show(&format!(":{path}"))?
            };
            Ok((before, after))
        } else {
            Ok((String::new(), String::new()))
        }
    }
}

fn parse_status(out: &str) -> Vec<FileEntry> {
    let mut files = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("1 ") {
            // "1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>"
            let parts: Vec<&str> = rest.splitn(8, ' ').collect();
            if parts.len() == 8 {
                files.push(entry(parts[0], parts[7]));
            }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            // "2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\t<orig>"
            let parts: Vec<&str> = rest.splitn(9, ' ').collect();
            if parts.len() == 9 {
                let path = parts[8].split('\t').next().unwrap_or("");
                files.push(entry(parts[0], path));
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            files.push(FileEntry {
                path: path.to_string(),
                staged: None,
                unstaged: Some('?'),
            });
        }
        // "u " (unmerged) and "! " (ignored) are skipped for now.
    }
    files
}

fn entry(xy: &str, path: &str) -> FileEntry {
    let mut chars = xy.chars();
    FileEntry {
        path: path.to_string(),
        staged: chars.next().filter(|c| *c != '.'),
        unstaged: chars.next().filter(|c| *c != '.'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let out = "\
1 .M N... 100644 100644 100644 abc def src/main.rs
1 M. N... 100644 100644 100644 abc def differ/src/app.rs
1 MM N... 100644 100644 100644 abc def both.rs
? new.txt
2 R. N... 100644 100644 100644 abc def R100 renamed.rs\torig.rs
";
        let files = parse_status(out);
        assert_eq!(files.len(), 5);

        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].staged, None);
        assert_eq!(files[0].unstaged, Some('M'));

        assert_eq!(files[1].path, "differ/src/app.rs");
        assert_eq!(files[1].staged, Some('M'));
        assert_eq!(files[1].unstaged, None);

        assert_eq!(files[2].staged, Some('M'));
        assert_eq!(files[2].unstaged, Some('M'));

        assert_eq!(files[3].path, "new.txt");
        assert_eq!(files[3].staged, None);
        assert_eq!(files[3].unstaged, Some('?'));

        assert_eq!(files[4].path, "renamed.rs");
        assert_eq!(files[4].staged, Some('R'));
    }
}
