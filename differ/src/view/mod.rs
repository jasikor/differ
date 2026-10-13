//! Rendering: the file list plus the two-pane diff.

mod files;
mod gutter;
mod panes;
pub mod theme;

use ratatui::{
    layout::{Constraint, Layout},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use theme::GAP;

/// Width of the file-list column.
const FILES_W: u16 = 32;

pub fn draw(f: &mut Frame, app: &App) {
    let [main, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let [files_col, old, gap, new] = Layout::horizontal([
        Constraint::Length(FILES_W),
        Constraint::Min(0),
        Constraint::Length(GAP),
        Constraint::Min(0),
    ])
    .areas(main);

    let [files_head, files_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(files_col);
    let [old_head, old_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(old);
    let [_, gap_body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(gap);
    let [new_head, new_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(new);

    let old_w = old_body.width as usize;
    let new_w = new_body.width as usize;
    let scroll = app.scroll;
    let right_scroll = (scroll as i32 + app.opts.delta).max(0) as u16;

    let repo_name = app
        .repo
        .as_ref()
        .and_then(|r| r.root().file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    f.render_widget(
        Paragraph::new(format!(" {repo_name}")).style(theme::dim_style()),
        files_head,
    );
    files::draw(f, &app.files, app.selected, app.file_scroll, files_body);

    f.render_widget(Paragraph::new(" old").style(theme::dim_style()), old_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, true, old_w)).scroll((scroll, 0)),
        old_body,
    );

    f.render_widget(Paragraph::new(" new").style(theme::dim_style()), new_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, false, new_w)).scroll((right_scroll, 0)),
        new_body,
    );

    let (gutter_lines, ovs, clears) = gutter::build_gutter(&app.rows, &app.opts, old_w, new_w);
    f.render_widget(Paragraph::new(gutter_lines).scroll((scroll, 0)), gap_body);
    gutter::draw_overlays(f, &ovs, &clears, old_body, new_body, scroll);

    f.render_widget(
        Paragraph::new(help_line(app)).style(theme::dim_style()),
        help,
    );
}

fn help_line(app: &App) -> String {
    let path = app.selected_path().unwrap_or("(no file)");
    let msg = if app.message.is_empty() {
        String::new()
    } else {
        format!("   {}", app.message)
    };
    format!(" {path}   j/k files  n/p hunk  J/K scroll  ,/. offset  R reload  q quit{msg}")
}
