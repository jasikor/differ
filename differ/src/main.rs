mod app;
mod git;
mod input;
mod model;
mod view;

use std::io;

use ratatui::crossterm::event::{self, Event};

use app::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| view::draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            if input::handle(app, key.code) {
                return Ok(());
            }
        }
    }
}
