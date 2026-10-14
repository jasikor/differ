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
    // Only the visible regions get a column: the file panel can be toggled off
    // (`app.show_files`) and single-pane zoom hides one diff pane (with the
    // gutter and both rulers). Omitted regions collapse to zero-size rects.
    let zero = Rect::new(0, 0, 0, 0);
    let show_old = app.zoom != Some(Focus::New);
    let show_new = app.zoom != Some(Focus::Old);
    let show_gap = app.zoom.is_none();

    let mut constraints: Vec<Constraint> = Vec::new();
    if app.show_files {
        constraints.push(Constraint::Length(FILES_W));
    }
    if show_old {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Min(0));
    }
    if show_gap {
        constraints.push(Constraint::Length(gap_w));
    }
    if show_new {
        constraints.push(Constraint::Min(0));
        constraints.push(Constraint::Length(1));
    }
    let cols = Layout::horizontal(constraints).split(main);
    let mut i = 0;
    let mut take = |present: bool| {
        if present {
            let rect = cols[i];
            i += 1;
            rect
        } else {
            zero
        }
    };
    let files_col = take(app.show_files);
    let old_ruler = take(show_old);
    let old = take(show_old);
    let gap = take(show_gap);
    let new = take(show_new);
    let new_ruler = take(show_new);

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
    app.viewport = main.height.saturating_sub(1) as usize;

    let old_w = old_body.width as usize;
    let new_w = new_body.width as usize;
    let old_top = app.old_top;
    let new_top = app.new_top;
    let old_kinds = model::side_kinds(&app.rows, true);
    let new_kinds = model::side_kinds(&app.rows, false);
    app.old_code_w = panes::code_width(old_w, old_kinds.len(), true);
    app.new_code_w = panes::code_width(new_w, new_kinds.len(), false);
    // A resize (or a zoom / panel toggle) changes the widths outside the
    // cursor-moving commands, so re-clamp the horizontal scroll every frame.
    app.keep_cols_visible();

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
    if app.show_files {
        f.render_widget(
            Paragraph::new(format!(" {repo_name}")).style(files_style),
            files_head,
        );
        files::draw(
            f,
            &app.tree,
            app.file_row.get(app.selected).copied(),
            app.file_scroll,
            files_body,
        );
    }

    f.render_widget(Paragraph::new(" old").style(old_style), old_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, true, old_w, app.left(true)))
            .scroll((old_top as u16, 0)),
        old_body,
    );
    f.render_widget(Paragraph::new(" new").style(new_style), new_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, false, new_w, app.left(false)))
            .scroll((new_top as u16, 0)),
        new_body,
    );

    ruler::draw(f, old_ruler_body, &old_kinds, old_top as u16);
    ruler::draw(f, new_ruler_body, &new_kinds, new_top as u16);

    let (gutter_lines, ovs, clears) = gutter::build_gutter(
        &app.rows,
        app.pair_shift(),
        gap_w,
        old_w,
        new_w,
        app.left(true),
        app.left(false),
    );
    f.render_widget(
        Paragraph::new(gutter_lines).scroll((old_top as u16, 0)),
        gap_body,
    );
    gutter::draw_overlays(
        f,
        &ovs,
        &clears,
        old_body,
        new_body,
        old_top as u16,
        app.zoom.is_some(),
    );

    // Cursors (topmost, so they read clearly).
    let old_off = panes::num_width(old_kinds.len()) + 1;
    draw_cursor(
        f,
        old_body,
        old_off + app.cursor_col(true).saturating_sub(app.left(true)),
        app.old_cursor,
        old_top,
        app.focus == Focus::Old,
    );
    draw_cursor(
        f,
        new_body,
        theme::NEW_MARGIN + app.cursor_col(false).saturating_sub(app.left(false)),
        app.new_cursor,
        new_top,
        app.focus == Focus::New,
    );

    f.render_widget(
        Paragraph::new(help_line(app, old_kinds.len())).style(theme::dim_style()),
        help,
    );
}

/// Highlight the cursor cell (the cursor line's code at its column) plus a very
/// faint tint on the whole cursor line. The active pane is drawn brighter than
/// the inactive one. `x_off` is the code column of the cursor cell (the pane's
/// code offset plus the cursor column minus the horizontal scroll).
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
        " {path}  {pct}% ({cur}/{old_total})  [{focus}]   ^H/^L focus  h/j/k/l move  w/b/e W/B/E word  0/^/$ line  ]c/[c change  gg/G top/bot  zz/zt/zb cursor  ^E/^Y scroll  ^Z zoom  Spc e panel  PgUp/PgDn  R reload  q quit{msg}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{test_app, Focus};
    use crate::model::DiffRow;
    use ratatui::{backend::TestBackend, Terminal};

    /// The single cursor block's x, and the text of the old pane's only body row.
    fn render(app: &mut App, w: u16) -> (u16, String) {
        let mut term = Terminal::new(TestBackend::new(w, 3)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer();
        let mut block_x = None;
        for x in 0..w {
            for y in 0..3u16 {
                if buf[(x, y)].bg == theme::CURSOR_ACTIVE {
                    block_x = Some(x);
                }
            }
        }
        let row: String = (0..w).map(|x| buf[(x, 1)].symbol().to_string()).collect();
        (block_x.expect("a cursor block"), row)
    }

    #[test]
    fn horizontal_scroll_pans_the_code_and_keeps_the_cursor_visible() {
        // A long context line, no file panel, in a 24-column terminal.
        let mut app = test_app(vec![DiffRow::Ctx("0123456789".into())], 5);
        app.show_files = false;
        app.focus = Focus::Old;
        // First draw settles the code width, then jump to the line end.
        render(&mut app, 24);
        app.line_end();

        let (block_x, row) = render(&mut app, 24);
        // The window starts at `col + 1 - code_w`; the block sits on the last
        // visible column. Old body starts at x = 1 (a 1-column ruler), plus the
        // 2-column number margin.
        assert_eq!(app.cursor_col(true), 9);
        assert_eq!(app.left(true), 9 + 1 - app.old_code_w);
        assert_eq!(
            block_x,
            1 + 2 + (app.cursor_col(true) - app.left(true)) as u16
        );
        // The old pane's code window shows the tail, not the head. (The row also
        // contains the new pane, which is not scrolled.)
        let old_code: String = row.chars().skip(3).take(app.old_code_w).collect();
        assert_eq!(old_code, "456789");
    }
}
