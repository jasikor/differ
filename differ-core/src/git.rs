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
    /// A revision spec that does not name a commit.
    UnknownRevision {
        spec: String,
    },
    /// Two revisions with no common ancestor.
    NoMergeBase {
        from: String,
        to: String,
    },
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
            Error::UnknownRevision { spec } => write!(f, "unknown revision: {spec}"),
            Error::NoMergeBase { from, to } => {
                write!(f, "no merge base between {from} and {to}")
            }
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

/// How a changed path differs: the two `git status` columns, or the single
/// change code a diff between revisions reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The index (`staged`) and worktree (`unstaged`) columns; absent when that
    /// side is unchanged.
    Worktree {
        staged: Option<char>,
        unstaged: Option<char>,
    },
    /// One code from `git diff --name-status`: `M`, `A`, `D`, `R`, `C` or `T`.
    Range(char),
}

impl Status {
    /// The change code, whichever shape the status has.
    pub fn code(self) -> char {
        match self {
            Status::Range(code) => code,
            Status::Worktree { staged, unstaged } => unstaged.or(staged).unwrap_or('.'),
        }
    }
}

/// A single changed path.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    /// Where the old side is read from when it is not `path` (a rename/copy).
    pub old_path: Option<String>,
    pub status: Status,
}

impl FileEntry {
    /// An entry from `git status`'s two-column state.
    pub fn worktree(path: impl Into<String>, staged: Option<char>, unstaged: Option<char>) -> Self {
        FileEntry {
            path: path.into(),
            old_path: None,
            status: Status::Worktree { staged, unstaged },
        }
    }

    /// An entry from a diff between two revisions.
    pub fn range(path: impl Into<String>, old_path: Option<String>, code: char) -> Self {
        FileEntry {
            path: path.into(),
            old_path,
            status: Status::Range(code),
        }
    }

    /// The path the old side of the diff lives at.
    pub fn old_path(&self) -> &str {
        self.old_path.as_deref().unwrap_or(&self.path)
    }
}

/// A revision already in the repository, resolved to a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rev {
    /// The commit the spec resolved to.
    pub commit: String,
    /// What to call this side in the UI.
    pub label: String,
}

impl Rev {
    fn new(spec: &str, commit: &str) -> Self {
        Rev {
            commit: commit.to_string(),
            label: spec.to_string(),
        }
    }
}

/// What a diff compares: the worktree (as `git status` sees it), two commits,
/// or one commit against the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    /// The index and worktree, mirroring `git status`. The default.
    Worktree,
    /// Two checked-in revisions.
    Revs { from: Rev, to: Rev },
    /// A checked-in revision against the working tree (`git diff <rev>`).
    RevToWorktree { from: Rev },
}

impl Comparison {
    /// A short description for headings and the status line.
    pub fn label(&self) -> String {
        match self {
            Comparison::Worktree => "working tree".to_string(),
            Comparison::Revs { from, to } => format!("{}..{}", from.label, to.label),
            Comparison::RevToWorktree { from } => format!("{}..working tree", from.label),
        }
    }

    /// The labels of the two panes, or `None` in the worktree view, where each
    /// file's own status names its sides.
    pub fn side_labels(&self) -> Option<(String, String)> {
        match self {
            Comparison::Worktree => None,
            Comparison::Revs { from, to } => Some((rev_label(from), rev_label(to))),
            Comparison::RevToWorktree { from } => {
                Some((rev_label(from), "working tree".to_string()))
            }
        }
    }
}

/// A revision with its short commit, e.g. `v0.2.0 (1a2b3c4)`.
fn rev_label(rev: &Rev) -> String {
    format!("{} ({})", rev.label, short_commit(&rev.commit))
}

/// The first seven characters of a commit hash (hex, so byte-safe).
fn short_commit(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
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

    /// The changed paths of a comparison. The worktree view mirrors `git
    /// status`; between revisions it is `git diff --name-status`, with renames
    /// detected.
    pub fn changes(&self, what: &Comparison) -> Result<Vec<FileEntry>> {
        match what {
            Comparison::Worktree => self.status(),
            Comparison::Revs { from, to } => self.diff_names(&[&from.commit, &to.commit]),
            Comparison::RevToWorktree { from } => self.diff_names(&[&from.commit]),
        }
    }

    fn diff_names(&self, commits: &[&str]) -> Result<Vec<FileEntry>> {
        let mut args = vec!["diff", "--name-status", "-z", "-M"];
        args.extend_from_slice(commits);
        Ok(parse_name_status(&self.git(&args)?))
    }

    /// `git show <spec>` (e.g. `:path` for the index, `HEAD:path` for HEAD).
    pub fn show(&self, spec: &str) -> Result<String> {
        self.git(&["show", spec])
    }

    pub fn worktree(&self, path: &str) -> Result<String> {
        std::fs::read_to_string(self.root.join(path)).map_err(Error::Io)
    }

    /// Before/after text for a changed file. In the worktree view the sides
    /// mirror `git status`: unstaged = index ↔ worktree, staged = HEAD ↔ index.
    /// Between revisions they are the file at each side. An added file has an
    /// empty before, a deleted one an empty after.
    pub fn before_after(&self, what: &Comparison, entry: &FileEntry) -> Result<(String, String)> {
        match what {
            Comparison::Worktree => self.worktree_before_after(entry),
            Comparison::Revs { from, to } => {
                let kind = entry.status.code();
                let before = if kind == 'A' {
                    String::new()
                } else {
                    self.at(&from.commit, entry.old_path())?
                };
                let after = if kind == 'D' {
                    String::new()
                } else {
                    self.at(&to.commit, &entry.path)?
                };
                Ok((before, after))
            }
            Comparison::RevToWorktree { from } => {
                let kind = entry.status.code();
                let before = if kind == 'A' {
                    String::new()
                } else {
                    self.at(&from.commit, entry.old_path())?
                };
                let after = if kind == 'D' {
                    String::new()
                } else {
                    self.worktree(&entry.path)?
                };
                Ok((before, after))
            }
        }
    }

    /// One side of a revision comparison: the file at `path` in `commit`.
    fn at(&self, commit: &str, path: &str) -> Result<String> {
        self.show(&format!("{commit}:{path}"))
    }

    fn worktree_before_after(&self, entry: &FileEntry) -> Result<(String, String)> {
        let path = &entry.path;
        match entry.status {
            Status::Worktree {
                unstaged: Some(y), ..
            } => {
                // A rename with unstaged edits is already staged, so the index
                // side is the file under its current name.
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
            }
            Status::Worktree {
                staged: Some(x), ..
            } => {
                // A staged rename's old side lives at the origin path.
                let before = if x == 'A' {
                    String::new()
                } else {
                    self.show(&format!("HEAD:{}", entry.old_path()))?
                };
                let after = if x == 'D' {
                    String::new()
                } else {
                    self.show(&format!(":{path}"))?
                };
                Ok((before, after))
            }
            _ => Ok((String::new(), String::new())),
        }
    }

    /// Resolve a revision spec into a [`Comparison`]:
    ///
    /// - `A..B` — two commits (`A` and `B` default to `HEAD` when omitted);
    /// - `A...B` — from their merge base to `B`;
    /// - `A` — `A` against the working tree;
    /// - empty — the worktree view.
    pub fn comparison(&self, spec: &str) -> Result<Comparison> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Ok(Comparison::Worktree);
        }
        if let Some((a, b)) = spec.split_once("...") {
            let (a, b) = (or_head(a), or_head(b));
            let commit = self.merge_base(a, b)?;
            return Ok(Comparison::Revs {
                from: Rev {
                    commit,
                    label: format!("merge-base({a}, {b})"),
                },
                to: self.rev(b)?,
            });
        }
        if let Some((a, b)) = spec.split_once("..") {
            return Ok(Comparison::Revs {
                from: self.rev(or_head(a))?,
                to: self.rev(or_head(b))?,
            });
        }
        Ok(Comparison::RevToWorktree {
            from: self.rev(spec)?,
        })
    }

    /// Resolve one revision spec to a commit.
    pub fn rev(&self, spec: &str) -> Result<Rev> {
        let unknown = || Error::UnknownRevision {
            spec: spec.to_string(),
        };
        let out = self
            .git(&["rev-parse", "--verify", &format!("{spec}^{{commit}}")])
            .map_err(|_| unknown())?;
        let commit = out.trim();
        if commit.is_empty() {
            return Err(unknown());
        }
        Ok(Rev::new(spec, commit))
    }

    fn merge_base(&self, a: &str, b: &str) -> Result<String> {
        // Resolve both first so an unknown revision gets its own message.
        let from = self.rev(a)?;
        let to = self.rev(b)?;
        let failure = || Error::NoMergeBase {
            from: a.to_string(),
            to: b.to_string(),
        };
        let out = self
            .git(&["merge-base", &from.commit, &to.commit])
            .map_err(|_| failure())?;
        let base = out.trim();
        if base.is_empty() {
            return Err(failure());
        }
        Ok(base.to_string())
    }
}

/// An empty side of `..` means `HEAD`, as in git's own revision syntax.
fn or_head(spec: &str) -> &str {
    if spec.is_empty() {
        "HEAD"
    } else {
        spec
    }
}

fn parse_status(out: &str) -> Vec<FileEntry> {
    let mut files = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("1 ") {
            // "1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>"
            let parts: Vec<&str> = rest.splitn(8, ' ').collect();
            if parts.len() == 8 {
                files.push(entry(parts[0], parts[7], None));
            }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            // "2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\t<orig>"
            let parts: Vec<&str> = rest.splitn(9, ' ').collect();
            if parts.len() == 9 {
                let (path, orig) = match parts[8].split_once('\t') {
                    Some((path, orig)) => (path, Some(orig.to_string())),
                    None => (parts[8], None),
                };
                files.push(entry(parts[0], path, orig));
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            files.push(FileEntry::worktree(path, None, Some('?')));
        }
        // "u " (unmerged) and "! " (ignored) are skipped for now.
    }
    files
}

fn entry(xy: &str, path: &str, old_path: Option<String>) -> FileEntry {
    let mut chars = xy.chars();
    let staged = chars.next().filter(|c| *c != '.');
    let unstaged = chars.next().filter(|c| *c != '.');
    FileEntry {
        path: path.to_string(),
        old_path,
        status: Status::Worktree { staged, unstaged },
    }
}

/// Parse `git diff --name-status -z` output: `STATUS\0path\0`, with renames and
/// copies as `R100\0old\0new\0`.
fn parse_name_status(out: &str) -> Vec<FileEntry> {
    let mut files = Vec::new();
    let mut parts = out.split('\0');
    while let Some(code) = parts.next() {
        if code.is_empty() {
            continue;
        }
        let kind = code.chars().next().unwrap_or('M');
        if kind == 'R' || kind == 'C' {
            let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
                break;
            };
            files.push(FileEntry::range(new, Some(old.to_string()), kind));
        } else {
            let Some(path) = parts.next() else { break };
            files.push(FileEntry::range(path, None, kind));
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command as Process;

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
        assert_eq!(
            files[0].status,
            Status::Worktree {
                staged: None,
                unstaged: Some('M')
            }
        );

        assert_eq!(files[1].path, "differ/src/app.rs");
        assert_eq!(
            files[1].status,
            Status::Worktree {
                staged: Some('M'),
                unstaged: None
            }
        );

        assert_eq!(
            files[2].status,
            Status::Worktree {
                staged: Some('M'),
                unstaged: Some('M')
            }
        );

        assert_eq!(files[3].path, "new.txt");
        assert_eq!(
            files[3].status,
            Status::Worktree {
                staged: None,
                unstaged: Some('?')
            }
        );

        assert_eq!(files[4].path, "renamed.rs");
        assert_eq!(files[4].old_path.as_deref(), Some("orig.rs"));
        assert_eq!(
            files[4].status,
            Status::Worktree {
                staged: Some('R'),
                unstaged: None
            }
        );
    }

    #[test]
    fn parses_name_status_with_renames_and_copies() {
        let out = "A\0new.txt\0M\0both.rs\0R100\0old.rs\0renamed.rs\0C075\0source.rs\0copy.rs\0D\0gone.rs\0";
        let files = parse_name_status(out);
        let summary: Vec<(&str, Option<&str>, char)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.old_path.as_deref(), f.status.code()))
            .collect();
        assert_eq!(
            summary,
            [
                ("new.txt", None, 'A'),
                ("both.rs", None, 'M'),
                ("renamed.rs", Some("old.rs"), 'R'),
                ("copy.rs", Some("source.rs"), 'C'),
                ("gone.rs", None, 'D'),
            ]
        );
    }

    // ---- tests against a throwaway repository ----

    /// A temporary repo with a deterministic identity, so tests never touch the
    /// caller's configuration or leave anything behind.
    struct Sandbox {
        dir: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("differ-git-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let sandbox = Sandbox { dir };
            sandbox.git(&["init", "-q", "-b", "main"]);
            sandbox.git(&["config", "user.email", "differ@example.com"]);
            sandbox.git(&["config", "user.name", "differ tests"]);
            sandbox.git(&["config", "commit.gpgsign", "false"]);
            sandbox
        }

        fn git(&self, args: &[&str]) -> String {
            let out = Process::new("git")
                .current_dir(&self.dir)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap()
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.dir.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, text).unwrap();
        }

        fn remove(&self, path: &str) {
            fs::remove_file(self.dir.join(path)).unwrap();
        }

        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", message]);
            self.git(&["rev-parse", "HEAD"]).trim().to_string()
        }

        fn tag(&self, name: &str) {
            self.git(&["tag", name]);
        }

        fn repo(&self) -> Repo {
            Repo::discover_in(Some(&self.dir)).unwrap()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn entry<'a>(files: &'a [FileEntry], path: &str) -> &'a FileEntry {
        files.iter().find(|f| f.path == path).unwrap_or_else(|| {
            panic!(
                "no entry for {path} in {:?}",
                files.iter().map(|f| &f.path).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn a_range_reports_added_modified_deleted_and_renamed_files() {
        let sandbox = Sandbox::new("range");
        sandbox.write("mod.txt", "a\n");
        sandbox.write("gone.txt", "b\n");
        // Rename detection is line-based, so keep most lines identical.
        sandbox.write("old.txt", "one\ntwo\nthree\nfour\nfive\n");
        sandbox.write("keep.txt", "same\n");
        sandbox.commit("first");
        sandbox.tag("v1");

        sandbox.write("mod.txt", "a2\n");
        sandbox.remove("gone.txt");
        sandbox.git(&["mv", "old.txt", "new.txt"]);
        sandbox.write("new.txt", "one\ntwo\nTHREE\nfour\nfive\n");
        sandbox.write("added.txt", "c\n");
        sandbox.commit("second");

        let repo = sandbox.repo();
        let range = repo.comparison("v1..HEAD").unwrap();
        let files = repo.changes(&range).unwrap();
        assert_eq!(files.len(), 4);

        assert_eq!(entry(&files, "added.txt").status, Status::Range('A'));
        assert_eq!(entry(&files, "mod.txt").status, Status::Range('M'));
        assert_eq!(entry(&files, "gone.txt").status, Status::Range('D'));
        let renamed = entry(&files, "new.txt");
        assert_eq!(renamed.status, Status::Range('R'));
        assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));

        assert_eq!(
            repo.before_after(&range, entry(&files, "mod.txt")).unwrap(),
            ("a\n".to_string(), "a2\n".to_string())
        );
        assert_eq!(
            repo.before_after(&range, entry(&files, "added.txt"))
                .unwrap(),
            (String::new(), "c\n".to_string())
        );
        assert_eq!(
            repo.before_after(&range, entry(&files, "gone.txt"))
                .unwrap(),
            ("b\n".to_string(), String::new())
        );
        // The old side of a rename comes from its origin path.
        assert_eq!(
            repo.before_after(&range, renamed).unwrap(),
            (
                "one\ntwo\nthree\nfour\nfive\n".to_string(),
                "one\ntwo\nTHREE\nfour\nfive\n".to_string()
            )
        );
    }

    #[test]
    fn a_single_revision_compares_against_the_working_tree() {
        let sandbox = Sandbox::new("worktree");
        sandbox.write("tracked.txt", "base\n");
        sandbox.commit("first");
        sandbox.tag("v1");

        sandbox.write("tracked.txt", "edited\n");
        sandbox.write("untracked.txt", "new\n");

        let repo = sandbox.repo();
        let range = repo.comparison("v1").unwrap();
        assert!(matches!(range, Comparison::RevToWorktree { .. }));
        let files = repo.changes(&range).unwrap();
        // Changes since the tag: untracked files have no revision to compare.
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "tracked.txt");
        assert_eq!(files[0].status, Status::Range('M'));
        assert_eq!(
            repo.before_after(&range, &files[0]).unwrap(),
            ("base\n".to_string(), "edited\n".to_string())
        );
    }

    #[test]
    fn three_dots_starts_from_the_merge_base() {
        let sandbox = Sandbox::new("merge-base");
        sandbox.write("shared.txt", "base\n");
        let base = sandbox.commit("base");
        sandbox.tag("base");

        sandbox.git(&["checkout", "-q", "-b", "topic"]);
        sandbox.write("topic.txt", "topic\n");
        sandbox.commit("topic");

        sandbox.git(&["checkout", "-q", "main"]);
        sandbox.write("main.txt", "main\n");
        sandbox.commit("main");

        let repo = sandbox.repo();
        let range = repo.comparison("main...topic").unwrap();
        let Comparison::Revs { from, .. } = &range else {
            panic!("expected two revisions, got {range:?}");
        };
        assert_eq!(from.commit, base);
        assert!(from.label.starts_with("merge-base("), "{}", from.label);

        // main's own commit is not part of the comparison.
        let paths: Vec<String> = repo
            .changes(&range)
            .unwrap()
            .iter()
            .map(|f| f.path.clone())
            .collect();
        assert_eq!(paths, ["topic.txt"]);
    }

    #[test]
    fn comparison_specs_parse_into_three_shapes() {
        let sandbox = Sandbox::new("specs");
        sandbox.write("a.txt", "a\n");
        sandbox.commit("first");
        sandbox.tag("v1");
        let repo = sandbox.repo();

        assert_eq!(repo.comparison("").unwrap(), Comparison::Worktree);
        assert_eq!(repo.comparison("   ").unwrap(), Comparison::Worktree);
        assert!(matches!(
            repo.comparison("v1").unwrap(),
            Comparison::RevToWorktree { from } if from.label == "v1"
        ));
        assert!(matches!(
            repo.comparison("v1..HEAD").unwrap(),
            Comparison::Revs { from, to } if from.label == "v1" && to.label == "HEAD"
        ));
        // An empty side of `..` means HEAD, as in git.
        assert!(matches!(
            repo.comparison("v1..").unwrap(),
            Comparison::Revs { to, .. } if to.label == "HEAD"
        ));
        assert!(matches!(
            repo.comparison("..HEAD").unwrap(),
            Comparison::Revs { from, .. } if from.label == "HEAD"
        ));

        let err = repo.comparison("no-such-rev").unwrap_err();
        assert!(err.to_string().contains("unknown revision"), "{err}");
        let err = repo.comparison("v1...no-such-rev").unwrap_err();
        assert!(err.to_string().contains("unknown revision"), "{err}");
    }

    #[test]
    fn unrelated_histories_have_no_merge_base() {
        let sandbox = Sandbox::new("no-base");
        sandbox.write("a.txt", "a\n");
        sandbox.commit("main");
        sandbox.git(&["checkout", "-q", "--orphan", "other"]);
        sandbox.write("b.txt", "b\n");
        sandbox.commit("other");

        let err = sandbox.repo().comparison("main...other").unwrap_err();
        assert!(err.to_string().contains("no merge base"), "{err}");
    }

    #[test]
    fn a_staged_rename_reads_the_old_side_from_its_origin() {
        let sandbox = Sandbox::new("staged-rename");
        sandbox.write("old.txt", "content\n");
        sandbox.commit("first");
        sandbox.git(&["mv", "old.txt", "new.txt"]);

        let repo = sandbox.repo();
        let files = repo.status().unwrap();
        let renamed = files.iter().find(|f| f.path == "new.txt").unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
        assert_eq!(
            repo.before_after(&Comparison::Worktree, renamed).unwrap(),
            ("content\n".to_string(), "content\n".to_string())
        );
    }

    #[test]
    fn binary_content_surfaces_as_an_error() {
        let sandbox = Sandbox::new("binary");
        sandbox.write("blob.bin", "text\n");
        sandbox.commit("first");
        sandbox.tag("v1");
        fs::write(sandbox.dir.join("blob.bin"), [0u8, 159, 146, 150]).unwrap();
        sandbox.commit("binary");

        let repo = sandbox.repo();
        let range = repo.comparison("v1..HEAD").unwrap();
        let files = repo.changes(&range).unwrap();
        assert_eq!(files.len(), 1);
        assert!(matches!(
            repo.before_after(&range, &files[0]),
            Err(Error::NonUtf8)
        ));
    }
}
