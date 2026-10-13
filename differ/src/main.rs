mod app;
mod git;
mod model;
mod view;

use std::io;

use ratatui::crossterm::event::{self, Event, KeyCode};

use app::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app = App::new();
    let result = run(&mut terminal, &app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &App) -> io::Result<()> {
    loop {
        terminal.draw(|f| view::draw(f, app))?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                _ => {}
            }
        }
    }
}
