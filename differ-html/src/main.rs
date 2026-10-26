//! `differ-html`: render diff (or two files) as a single self-contained page.
//!
//! Experimental graphical front end. It shares the diff model, git access and
//! file ordering with the TUI through `differ-core`; only the rendering differs.

mod render;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use differ_core::git::{FileEntry, Repo};
use differ_core::model;
use differ_core::tree;

use crate::render::Diff;

/// Files with more rows than this are listed but not rendered: the page keeps
/// the browser responsive while the idea is being evaluated.
const DEFAULT_MAX_ROWS: usize = 4000;

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(options) => options,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

struct Options {
    /// `--repo`, or the current directory when no files were given.
    repo: Option<PathBuf>,
    /// Positional arguments: two files to compare, or path filters in a repo.
    files: Vec<PathBuf>,
    out: String,
    open: bool,
    max_rows: usize,
    /// Wrap long lines (default); `--no-wrap` clips them like the TUI.
    wrap: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        repo: None,
        files: Vec::new(),
        out: "differ.html".to_string(),
        open: false,
        max_rows: DEFAULT_MAX_ROWS,
        wrap: true,
    };
    let mut args = std::env::args().skip(1);
    let mut parse_options = true;
    while let Some(arg) = args.next() {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options {
            match arg.as_str() {
                "-r" | "--repo" => {
                    let value = args.next().ok_or("--repo needs a path")?;
                    options.repo = Some(PathBuf::from(value));
                    continue;
                }
                "-o" | "--out" => {
                    options.out = args.next().ok_or("--out needs a path")?;
                    continue;
                }
                "--open" => {
                    options.open = true;
                    continue;
                }
                "--no-wrap" => {
                    options.wrap = false;
                    continue;
                }
                "--max-rows" => {
                    let value = args.next().ok_or("--max-rows needs a number")?;
                    options.max_rows = value
                        .parse()
                        .map_err(|_| format!("--max-rows: not a number: {value}"))?;
                    continue;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other if other.starts_with('-') && other != "-" => {
                    return Err(format!("unknown argument: {other}"));
                }
                _ => {}
            }
        }
        options.files.push(PathBuf::from(arg));
    }
    if options.repo.is_none() && options.files.len() > 2 {
        return Err("expected two files to compare, or --repo with path filters".into());
    }
    Ok(options)
}

fn print_help() {
    println!(
        "differ-html - experimental graphical diff (static HTML)\n\n\
         usage:\n  \
         differ-html [--repo <path>] [path-filter ...] [-o <file>] [--open]\n  \
         differ-html <old-file> <new-file> [-o <file>] [--open]\n\n\
         options:\n  \
         -r, --repo <path>   repository to read (default: current directory)\n  \
         -o, --out <file>    output path, or - for stdout (default: differ.html)\n  \
         --open              open the page in the default browser\n  \
         --no-wrap           clip long lines instead of wrapping them\n  \
         --max-rows <n>      skip files with more than n diff rows (default: {DEFAULT_MAX_ROWS})"
    );
}

fn run(options: Options) -> Result<(), String> {
    let (diffs, source) = build(&options)?;
    let html = render::page(&diffs, &source, options.wrap);

    let path = Path::new(&options.out);
    if options.out == "-" {
        std::io::stdout()
            .write_all(html.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        fs::write(path, html).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("{}", path.display());
    }

    if options.open {
        open(path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
    }
    Ok(())
}

fn open(path: &Path) -> std::io::Result<()> {
    Command::new("xdg-open").arg(path).spawn().map(|_| ())
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn build(options: &Options) -> Result<(Vec<Diff>, String), String> {
    match (&options.repo, options.files.len()) {
        (None, 2) => {
            let old = &options.files[0];
            let new = &options.files[1];
            let old_text =
                fs::read_to_string(old).map_err(|e| format!("{}: {e}", old.display()))?;
            let new_text =
                fs::read_to_string(new).map_err(|e| format!("{}: {e}", new.display()))?;
            let diff = Diff {
                path: format!("{} → {}", old.display(), new.display()),
                old_label: base_name(old),
                new_label: base_name(new),
                rows: model::diff_rows(&old_text, &new_text),
                note: None,
            };
            Ok((vec![diff], "direct file comparison".to_string()))
        }
        _ => build_repo(options),
    }
}

fn build_repo(options: &Options) -> Result<(Vec<Diff>, String), String> {
    let repo = Repo::discover_in(options.repo.as_deref()).map_err(|e| e.to_string())?;
    let mut entries = repo.status().map_err(|e| e.to_string())?;
    if !options.files.is_empty() {
        let filters: Vec<String> = options
            .files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        entries.retain(|e| filters.iter().any(|f| e.path.ends_with(f.as_str())));
    }
    tree::order(&mut entries);

    let mut diffs = Vec::new();
    for entry in &entries {
        diffs.push(read_entry(&repo, entry, options.max_rows));
    }
    Ok((diffs, repo.root().display().to_string()))
}

fn read_entry(repo: &Repo, entry: &FileEntry, max_rows: usize) -> Diff {
    let (old_label, new_label) = render::side_labels(entry);
    let mut diff = Diff {
        path: entry.path.clone(),
        old_label,
        new_label,
        rows: Vec::new(),
        note: None,
    };
    match repo.before_after(entry) {
        Ok((old, new)) => {
            let rows = model::diff_rows(&old, &new);
            if rows.len() > max_rows {
                diff.note = Some(format!(
                    "{} diff rows — skipped (raise --max-rows to render it)",
                    rows.len()
                ));
            } else if rows.iter().all(|r| matches!(r, model::DiffRow::Ctx(_))) {
                diff.note = Some("no textual changes (mode or binary change?)".to_string());
            } else {
                diff.rows = rows;
            }
        }
        Err(e) => diff.note = Some(format!("{e}")),
    }
    diff
}
