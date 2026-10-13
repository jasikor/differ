mod app;
mod command;
mod git;
mod keymap;
mod model;
mod view;

use std::io;

use ratatui::crossterm::event::{self, Event};

use app::App;
use command::Outcome;
use keymap::{Keymap, Resolution};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let mut keymap = Keymap::default_bindings();
    let result = run(&mut terminal, &mut app, &mut keymap);
    ratatui::restore();
    result
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
