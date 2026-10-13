mod app;
mod command;
mod git;
mod keymap;
mod model;
mod view;

use std::io;
use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event};

use app::App;
use command::Outcome;
use keymap::{Keymap, Resolution};

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
    let result = run(&mut terminal, &mut app, &mut keymap);
    ratatui::restore();
    result
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
        terminal.draw(|f| view::draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            if let Resolution::Run(cmd) = keymap.feed(key.code) {
                if cmd.run(app) == Outcome::Quit {
                    return Ok(());
                }
            }
        }
    }
}
