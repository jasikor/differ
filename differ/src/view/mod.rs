//! Rendering: the file list, two overview rulers, and the two-pane diff.

mod files;
mod gutter;
mod panes;
mod ruler;
pub mod theme;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Color,
    text::Line,
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Focus, JumpState, Search};
use crate::model;

/// Width of the file-list column.
const FILES_W: u16 = 32;

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.help_open {
        draw_help(f, app);
        return;
    }
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

    // Search matches (under the cursor so the cursor line still reads).
    let old_off = panes::num_width(old_kinds.len()) + 1;
    if let Some(search) = app.search() {
        draw_search(f, app, search, old_body, new_body, old_off);
    }

    // Cursors (topmost, so they read clearly).
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

    // Jump labels paint over everything while `s` is choosing.
    if app.jump_state() == JumpState::Active {
        draw_jump_labels(f, app, old_body, new_body, old_off);
    }

    let (help_text, cursor_x) = help_line(app, old_kinds.len());
    f.render_widget(Paragraph::new(help_text).style(theme::dim_style()), help);
    // Show the terminal cursor while typing a prompt, so there is a caret.
    if let Some(col) = cursor_x {
        let x = (help.x + col).min(help.x + help.width.saturating_sub(1));
        f.set_cursor_position((x, help.y));
    }
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

/// Paint the `s` jump labels just after each visible match (flash's
/// `label.after`), tinting the matched text so the target reads clearly.
fn draw_jump_labels(f: &mut Frame, app: &App, old_body: Rect, new_body: Rect, old_off: usize) {
    let plen = app.jump_pattern_len() as isize;
    let buf = f.buffer_mut();
    for m in app.jump_matches() {
        let Some(label) = m.label else { continue };
        let (body, top, base, left) = if m.old_side {
            (old_body, app.old_top, old_off, app.left(true))
        } else {
            (new_body, app.new_top, theme::NEW_MARGIN, app.left(false))
        };
        if body.width == 0 {
            continue;
        }
        let row = m.line as isize - top as isize;
        if row < 0 || row >= body.height as isize {
            continue;
        }
        let y = body.y + row as u16;
        let start = base as isize + m.col as isize - left as isize;
        for i in 0..plen {
            let x = start + i;
            if x >= 0 && x < body.width as isize {
                buf[(body.x + x as u16, y)].set_bg(theme::JUMP_MATCH_BG);
            }
        }
        let x = start + plen;
        if x >= 0 && x < body.width as isize {
            let cell = &mut buf[(body.x + x as u16, y)];
            cell.set_char(label);
            cell.set_fg(theme::JUMP_FG);
            cell.set_bg(theme::JUMP_BG);
        }
    }
}

/// Tint every visible search match in **both** panes, brighter for the match
/// under the focused pane's cursor (vim's `Search` vs `CurSearch`).
fn draw_search(
    f: &mut Frame,
    app: &App,
    search: &Search,
    old_body: Rect,
    new_body: Rect,
    old_off: usize,
) {
    let buf = f.buffer_mut();
    for old_side in [true, false] {
        let (body, top, base, left) = if old_side {
            (old_body, app.old_top, old_off, app.left(true))
        } else {
            (new_body, app.new_top, theme::NEW_MARGIN, app.left(false))
        };
        if body.width == 0 {
            continue;
        }
        // The current match is the one under the focused pane's cursor.
        let current_pos = match (app.focus, old_side) {
            (Focus::Old, true) => Some((app.old_cursor, app.cursor_col(true))),
            (Focus::New, false) => Some((app.new_cursor, app.cursor_col(false))),
            _ => None,
        };
        for hit in search.hits(old_side) {
            let row = hit.line as isize - top as isize;
            if row < 0 || row >= body.height as isize {
                continue;
            }
            let y = body.y + row as u16;
            let current = current_pos == Some((hit.line, hit.col));
            for i in 0..hit.len {
                let x = base as isize + hit.col as isize - left as isize + i as isize;
                if x < 0 || x >= body.width as isize {
                    continue;
                }
                let cell = &mut buf[(body.x + x as u16, y)];
                if current {
                    cell.set_bg(theme::SEARCH_CURRENT_BG);
                    cell.set_fg(theme::SEARCH_CURRENT_FG);
                } else {
                    cell.set_bg(theme::SEARCH_BG);
                }
            }
        }
    }
}

/// The full-screen help page (`?`).
fn draw_help(f: &mut Frame, app: &App) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let visible = body.height as usize;
    let total = app.help_lines.len();
    let scroll = app.help_scroll.min(total.saturating_sub(visible));
    let lines: Vec<Line> = app
        .help_lines
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|l| Line::from(l.clone()))
        .collect();
    f.render_widget(Paragraph::new(lines).style(theme::dim_style()), body);
    f.render_widget(
        Paragraph::new(" differ keys    j/k scroll    ? / Esc close").style(theme::dim_style()),
        footer,
    );
}

/// The bottom line, plus the caret column when a prompt is active.
fn help_line(app: &App, old_total: usize) -> (String, Option<u16>) {
    // The search prompt owns the line while typing (vim's command line), so it
    // is never clipped. The `s` jump shows its labels inline instead of a
    // prompt, so the normal hints stay put.
    if app.search_input() {
        let pattern = app.search().map(|s| s.pattern.as_str()).unwrap_or("");
        let text = format!("/{pattern}");
        let col = 1 + pattern.chars().count();
        return (text, Some(col as u16));
    }
    // which-key: while a prefix is pending, show just that prefix's options.
    if !app.key_hint.is_empty() {
        return (format!(" {}", app.key_hint), None);
    }
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
    // Keep messages near the front so they are visible.
    let msg = if app.message.is_empty() {
        String::new()
    } else {
        format!("{}   ", app.message)
    };
    let text = format!(" {path}  {pct}% ({cur}/{old_total})  [{focus}]   {msg}? help   q quit");
    (text, None)
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

    #[test]
    fn jump_paints_labels_after_the_matches() {
        use ratatui::crossterm::event::KeyCode;
        let mut app = test_app(vec![DiffRow::Ctx("xaxa".into())], 5);
        app.show_files = false;
        app.focus = Focus::Old;

        let mut term = Terminal::new(TestBackend::new(24, 3)).unwrap();
        // First draw settles the code width, then start the jump.
        term.draw(|f| draw(f, &mut app)).unwrap();
        app.start_jump();
        app.jump_key(KeyCode::Char('a'));
        term.draw(|f| draw(f, &mut app)).unwrap();

        let buf = term.backend().buffer();
        // Labels are drawn after the match, in both panes now: the old pane's
        // matches at cols 1 and 3, and the new pane's too.
        let labels: Vec<(u16, char)> = (0..24)
            .filter(|&x| buf[(x, 1)].bg == theme::JUMP_BG)
            .map(|x| (x, buf[(x, 1)].symbol().chars().next().unwrap()))
            .collect();
        // x = body offset + number margin + label column.
        assert!(labels.contains(&(5, 'a')) && labels.contains(&(7, 's')));
        assert_eq!(labels.len(), 4);
        // The matched characters themselves are tinted in both panes.
        let matches: Vec<u16> = (0..24)
            .filter(|&x| buf[(x, 1)].bg == theme::JUMP_MATCH_BG)
            .collect();
        assert!(matches.contains(&4) && matches.contains(&6));
        assert_eq!(matches.len(), 4);
    }

    #[test]
    fn search_tints_matches_on_other_lines() {
        use ratatui::crossterm::event::KeyCode;
        let mut app = test_app(
            vec![
                DiffRow::Ctx("ctx".into()),
                DiffRow::Ctx("axa".into()),
                DiffRow::Ctx("axa".into()),
            ],
            5,
        );
        app.show_files = false;
        app.focus = Focus::Old;

        let mut term = Terminal::new(TestBackend::new(24, 5)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        app.start_search();
        app.search_key(KeyCode::Char('a'));
        term.draw(|f| draw(f, &mut app)).unwrap();

        // The preview moves the cursor onto line 1, so line 2's matches in both
        // panes keep the plain search tint: x = body offset + number margin +
        // col, at y 3.
        let buf = term.backend().buffer();
        let tinted: Vec<(u16, u16)> = (0..24)
            .flat_map(|x| (0..5u16).map(move |y| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].bg == theme::SEARCH_BG)
            .collect();
        // Both panes highlight: the old pane at x 3 and 5, and the new pane too.
        assert!(tinted.contains(&(3, 3)) && tinted.contains(&(5, 3)));
        assert_eq!(tinted.len(), 4);
    }

    #[test]
    fn the_search_prompt_replaces_the_help_line() {
        let mut app = test_app(vec![DiffRow::Ctx("abc".into())], 5);
        app.focus = Focus::Old;
        app.old_code_w = 40;
        app.start_search();
        app.search_key(ratatui::crossterm::event::KeyCode::Char('a'));
        let (text, caret) = help_line(&app, 1);
        assert_eq!(text, "/a");
        // The caret sits just past the typed pattern.
        assert_eq!(caret, Some(2));
        // Once committed, the normal hints come back (no caret).
        app.search_key(ratatui::crossterm::event::KeyCode::Enter);
        let (text, caret) = help_line(&app, 1);
        assert!(text.contains("? help"));
        assert_eq!(caret, None);
    }

    #[test]
    fn help_page_renders_and_key_hint_takes_the_status_line() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.help_lines = vec![
            "Panels".into(),
            "    Space e       toggle file panel".into(),
        ];
        app.help_open = true;
        let mut term = Terminal::new(TestBackend::new(60, 6)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let text: String = (0..6)
            .map(|y| {
                (0..60)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Panels"));
        assert!(text.contains("toggle file panel"));

        // With help closed, the which-key hint owns the bottom line.
        app.help_open = false;
        app.key_hint = "Space: e toggle file panel".into();
        let (line, caret) = help_line(&app, 1);
        assert!(line.contains("toggle file panel"));
        assert_eq!(caret, None);
    }
}
