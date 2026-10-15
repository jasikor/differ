mod app;
mod command;
mod git;
mod keymap;
mod model;
mod tree;
mod view;

use std::io;
use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event, KeyCode};

use app::App;
use command::Outcome;
use keymap::{Key, Keymap, Resolution};

fn main() -> io::Result<()> {
    let repo = match parse_args() {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let mut terminal = ratatui::init();
    let mut app = App::new(repo.as_deref());
    let mut keymap = Keymap::default_bindings();
    app.help_lines = build_help();
    let result = run(&mut terminal, &mut app, &mut keymap);
    ratatui::restore();
    result
}

/// The full-screen help page: related keys grouped on one row.
fn build_help() -> Vec<String> {
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

/// Parse `[--repo <path>]` (alias `-r`); `None` means the current directory.
fn parse_args() -> Result<Option<PathBuf>, String> {
    let mut repo = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-r" | "--repo" => {
                let value = args.next().ok_or("--repo needs a path")?;
                repo = Some(PathBuf::from(value));
            }
            "-h" | "--help" => {
                println!("differ - side-by-side git diff viewer\n\nusage: differ [--repo <path>]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(repo)
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
