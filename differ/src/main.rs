mod app;
mod command;
mod keymap;
mod view;

// The data layer is shared with the other front ends; keep the crate-local
// names so `crate::model`, `crate::git` and friends still resolve.
pub use differ_core::{git, model, symbol, tree};

use std::io;
use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event, KeyCode};

use app::App;
use command::Outcome;
use keymap::{Key, Keymap, Resolution};

fn main() -> io::Result<()> {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let mut terminal = ratatui::init();
    let mut app = match args {
        Args::Repo(repo) => App::new(repo.as_deref()),
        Args::Files(old, new) => App::compare(&old, &new),
    };
    let mut keymap = Keymap::default_bindings();
    app.help_lines = build_help(app.pair.is_none());
    let result = run(&mut terminal, &mut app, &mut keymap);
    ratatui::restore();
    result
}

/// The full-screen help page: related keys grouped on one row. `files_panel`
/// is false when two files are compared directly, where the file list (and its
/// keys) does not exist.
fn build_help(files_panel: bool) -> Vec<String> {
    const HELP: &[(&str, &str, &str)] = &[
        ("Panels", "Ctrl-h / Ctrl-l", "focus previous / next panel"),
        ("Panels", "Space e", "show/hide the file (git) panel"),
        ("Panels", "Ctrl-z", "zoom the focused pane"),
        ("Movement", "h / j / k / l", "move left / down / up / right"),
        ("Movement", "w / b / e", "forward / back / end of word"),
        (
            "Movement",
            "W / B / E",
            "same, by WORD (whitespace-delimited)",
        ),
        (
            "Movement",
            "0 / ^ / $",
            "line start / first non-blank / end",
        ),
        ("Movement", "gg / G", "top / bottom"),
        ("Movement", "PgUp / PgDn", "page up / down"),
        ("Changes", "]c / [c", "next / previous change"),
        (
            "Changes",
            "Ctrl-n / Ctrl-p",
            "next / previous change (alias)",
        ),
        ("Search", "/", "search the pane (literal, smartcase)"),
        ("Search", "n / N", "next / previous match"),
        ("Symbols", "gd / gr", "definition / references (ripgrep)"),
        ("Symbols", "Ctrl-o", "back after a symbol jump"),
        ("Jump", "s", "type a pattern, then a label to jump"),
        (
            "Files",
            "Space n / Space p",
            "next / previous file (any focus)",
        ),
        ("Files", "j / k", "move in the file list"),
        (
            "View",
            "Ctrl-e / Ctrl-y",
            "scroll the view a line (cursor stays)",
        ),
        (
            "View",
            "zz / zt / zb",
            "cursor line to middle / top / bottom",
        ),
        ("Application", "R", "reload git status and the current diff"),
        ("Application", "?", "this help (j/k scroll, Esc close)"),
        ("Application", "q", "quit"),
        ("Application", "Esc", "cancel a pending key"),
    ];
    let mut lines = Vec::new();
    let mut section = "";
    for (sec, keys, desc) in HELP {
        // Without a file list, drop its section and the panel toggle.
        if !files_panel && (*sec == "Files" || *keys == "Space e") {
            continue;
        }
        if *sec != section {
            if !section.is_empty() {
                lines.push(String::new());
            }
            lines.push((*sec).to_string());
            section = sec;
        }
        lines.push(format!("    {:<20} {}", keys, desc));
    }
    lines
}

/// What the command line asked for: browse a repository, or compare two files.
#[derive(Debug, PartialEq, Eq)]
enum Args {
    /// The repository start directory; `None` means the current directory.
    Repo(Option<PathBuf>),
    /// The old and new file to compare directly.
    Files(PathBuf, PathBuf),
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

/// Parse `[--repo <path>]` or `<old-file> <new-file>`. `--` ends option
/// parsing, so file names may start with a dash.
fn parse_args_from<I: Iterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut repo = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut options = true;
    let mut args = args;
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options {
            match arg.as_str() {
                "-r" | "--repo" => {
                    let value = args.next().ok_or("--repo needs a path")?;
                    repo = Some(PathBuf::from(value));
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
        files.push(PathBuf::from(arg));
    }
    match (repo, files.len()) {
        (Some(_), 1..) => Err("--repo cannot be combined with files to compare".to_string()),
        (Some(repo), _) => Ok(Args::Repo(Some(repo))),
        (None, 0) => Ok(Args::Repo(None)),
        (None, 2) => {
            let mut paths = files.into_iter();
            Ok(Args::Files(
                paths.next().ok_or("expected two files")?,
                paths.next().ok_or("expected two files")?,
            ))
        }
        (None, n) => Err(format!("expected two files to compare, got {n}")),
    }
}

fn print_help() {
    println!(
        "differ - side-by-side diff viewer\n\n\
         usage:\n  \
         differ [--repo <path>]\n  \
         differ <old-file> <new-file>"
    );
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    keymap: &mut Keymap,
) -> io::Result<()> {
    loop {
        // Refresh the which-key hint for any pending prefix, then draw.
        app.key_hint = keymap.hint();
        terminal.draw(|f| view::draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            let key = Key::from_event(key);
            // The symbol result list is modal (j/k move, Enter open, Esc close).
            if app.locations_active() {
                app.locations_key(key.code);
                continue;
            }
            // The help page captures keys while it is open (j/k scroll, close).
            if app.help_open {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.close_help(),
                    KeyCode::Char('j') | KeyCode::Down => app.help_scroll_by(1),
                    KeyCode::Char('k') | KeyCode::Up => app.help_scroll_by(-1),
                    _ => {}
                }
                continue;
            }
            // Modal input captures keys while active.
            if app.jump_active() {
                app.jump_key(key.code);
                continue;
            }
            if app.search_input() {
                app.search_key(key.code);
                continue;
            }
            if let Resolution::Run(cmd) = keymap.feed(key) {
                if cmd.run(app) == Outcome::Quit {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(list: &[&str]) -> Result<Args, String> {
        parse_args_from(list.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn no_arguments_browses_the_current_directory() {
        assert_eq!(parse(&[]).unwrap(), Args::Repo(None));
    }

    #[test]
    fn repo_flag_takes_a_path() {
        assert_eq!(
            parse(&["--repo", "some/repo"]).unwrap(),
            Args::Repo(Some("some/repo".into()))
        );
        assert_eq!(
            parse(&["-r", "some/repo"]).unwrap(),
            Args::Repo(Some("some/repo".into()))
        );
    }

    #[test]
    fn two_positional_arguments_compare_files() {
        assert_eq!(
            parse(&["old.txt", "new.txt"]).unwrap(),
            Args::Files("old.txt".into(), "new.txt".into())
        );
    }

    #[test]
    fn double_dash_allows_dashed_file_names() {
        assert_eq!(
            parse(&["--", "-old.txt", "-new.txt"]).unwrap(),
            Args::Files("-old.txt".into(), "-new.txt".into())
        );
    }

    #[test]
    fn rejects_the_wrong_number_of_files() {
        assert!(parse(&["one.txt"]).is_err());
        assert!(parse(&["a.txt", "b.txt", "c.txt"]).is_err());
    }

    #[test]
    fn rejects_repo_combined_with_files() {
        assert!(parse(&["--repo", "repo", "a.txt", "b.txt"]).is_err());
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse(&["--bogus"]).is_err());
    }

    #[test]
    fn repo_flag_needs_a_value() {
        assert!(parse(&["--repo"]).is_err());
    }
}
