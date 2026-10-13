//! Rendering: the file list, two overview rulers, and the two-pane diff.

mod files;
mod gutter;
mod panes;
mod ruler;
pub mod theme;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Color,
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Focus};
use crate::model;

/// Width of the file-list column.
const FILES_W: u16 = 32;

pub fn draw(f: &mut Frame, app: &mut App) {
    let [main, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let gap_w = gutter::gutter_width(&app.rows);
    let [files_col, old_ruler, old, gap, new, new_ruler] = Layout::horizontal([
        Constraint::Length(FILES_W),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(gap_w),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(main);

    let [files_head, files_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(files_col);
    let [_, old_ruler_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(old_ruler);
    let [old_head, old_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(old);
    let [_, gap_body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(gap);
    let [new_head, new_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(new);
    let [_, new_ruler_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(new_ruler);

    // Let the app know how many lines fit, for scrolling/clamping.
    app.viewport = old_body.height as usize;

    let old_w = old_body.width as usize;
    let new_w = new_body.width as usize;
    let old_top = app.old_top;
    let new_top = app.new_top;

    let (files_style, old_style, new_style) = match app.focus {
        Focus::Files => (
            theme::active_style(),
            theme::dim_style(),
            theme::dim_style(),
        ),
        Focus::Old => (
            theme::dim_style(),
            theme::active_style(),
            theme::dim_style(),
        ),
        Focus::New => (
            theme::dim_style(),
            theme::dim_style(),
            theme::active_style(),
        ),
    };

    let repo_name = app
        .repo
        .as_ref()
        .and_then(|r| r.root().file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    f.render_widget(
        Paragraph::new(format!(" {repo_name}")).style(files_style),
        files_head,
    );
    files::draw(f, &app.files, app.selected, app.file_scroll, files_body);

    f.render_widget(Paragraph::new(" old").style(old_style), old_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, true, old_w)).scroll((old_top as u16, 0)),
        old_body,
    );
    f.render_widget(Paragraph::new(" new").style(new_style), new_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, false, new_w)).scroll((new_top as u16, 0)),
        new_body,
    );

    let old_kinds = model::side_kinds(&app.rows, true);
    let new_kinds = model::side_kinds(&app.rows, false);
    ruler::draw(f, old_ruler_body, &old_kinds, old_top as u16);
    ruler::draw(f, new_ruler_body, &new_kinds, new_top as u16);

    let (gutter_lines, ovs, clears) =
        gutter::build_gutter(&app.rows, app.pair_shift(), gap_w, old_w, new_w);
    f.render_widget(
        Paragraph::new(gutter_lines).scroll((old_top as u16, 0)),
        gap_body,
    );
    gutter::draw_overlays(f, &ovs, &clears, old_body, new_body, old_top as u16);

    // Cursors (topmost, so they read clearly).
    let old_off = panes::num_width(old_kinds.len()) + 1;
    draw_cursor(
        f,
        old_body,
        old_off,
        app.old_cursor,
        old_top,
        app.focus == Focus::Old,
    );
    draw_cursor(
        f,
        new_body,
        0,
        app.new_cursor,
        new_top,
        app.focus == Focus::New,
    );

    f.render_widget(
        Paragraph::new(help_line(app, old_kinds.len())).style(theme::dim_style()),
        help,
    );
}

/// Highlight the cursor cell (the start of the cursor line's code) plus a very
/// faint tint on the whole cursor line. The active pane is drawn brighter than
/// the inactive one.
fn draw_cursor(f: &mut Frame, area: Rect, x_off: usize, cursor: usize, top: usize, active: bool) {
    let vp = area.height as usize;
    if vp == 0 || cursor < top || cursor >= top + vp {
        return;
    }
    let y = area.y + (cursor - top) as u16;
    let block = if active {
        theme::CURSOR_ACTIVE
    } else {
        theme::CURSOR_INACTIVE
    };
    let buf = f.buffer_mut();
    for x in area.x..area.x + area.width {
        let cell = &mut buf[(x, y)];
        // Lighten existing change tints (keep their hue) rather than
        // flattening them; only bare cells get the faint base.
        let bg = match cell.bg {
            Color::Reset => theme::CURSOR_LINE,
            c => theme::brighten(c, 0x0c),
        };
        cell.set_bg(bg);
    }
    let x = area.x + x_off as u16;
    if x < area.x + area.width {
        buf[(x, y)].set_bg(block);
    }
}

fn help_line(app: &App, old_total: usize) -> String {
    let path = app.selected_path().unwrap_or("(no file)");
    let cur = app.old_top.min(old_total);
    let pct = if old_total == 0 {
        100
    } else {
        cur * 100 / old_total
    };
    let focus = match app.focus {
        Focus::Files => "files",
        Focus::Old => "old",
        Focus::New => "new",
    };
    let msg = if app.message.is_empty() {
        String::new()
    } else {
        format!("   {}", app.message)
    };
    format!(
        " {path}  {pct}% ({cur}/{old_total})  [{focus}]   Tab focus  j/k move  n/p hunk  PgUp/PgDn  R reload  q quit{msg}"
    )
}
