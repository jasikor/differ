use ratatui::{widgets::Paragraph, Frame};

use crate::app::App;

/// Stage 1: a plain, full-screen placeholder that proves the git wiring works.
/// Stage 2 replaces this with the file-list + two-pane diff layout.
pub fn draw(f: &mut Frame, app: &App) {
    let mut text = String::from("differ - stage 1 skeleton\n\n");
    match &app.repo {
        Some(repo) => text.push_str(&format!("repo:  {}\n", repo.root().display())),
        None => text.push_str("repo:  (none)\n"),
    }
    text.push_str(&format!("files: {}\n\n", app.files.len()));

    for (i, file) in app.files.iter().enumerate().take(30) {
        let x = file.staged.unwrap_or('.');
        let y = file.unstaged.unwrap_or('.');
        text.push_str(&format!("  {i:>3}  {x}{y}  {}\n", file.path));
    }

    text.push_str(&format!("\n{}\n\npress q to quit", app.message));
    f.render_widget(Paragraph::new(text), f.area());
}
