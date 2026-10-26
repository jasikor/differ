//! Rendering: the file list, two overview rulers, and the two-pane diff.

mod files;
mod gutter;
mod panes;
mod ruler;
pub mod theme;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::app::{App, Focus, JumpState, Search};
use crate::model;

/// Width of the left column: changed files on top, versions below.
const LEFT_W: u16 = 52;

/// Columns the branch graph may take before it is clipped (the commit marker
/// at the end is always kept).
const GRAPH_W: usize = 12;

/// Build metadata stamped by `build.rs`.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMIT: &str = env!("DIFFER_GIT_HASH");
const COMMIT_TIME: &str = env!("DIFFER_COMMIT_TIME");

/// The left header's repository label: the worktree directory, then the branch
/// it has checked out (`differ @ main`). Either part may be missing.
fn repo_label(repo_name: &str, branch: &str) -> String {
    match (repo_name.is_empty(), branch.is_empty()) {
        (true, _) => branch.to_string(),
        (false, true) => repo_name.to_string(),
        (false, false) => format!("{repo_name} @ {branch}"),
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.help_open {
        draw_help(f, app);
        return;
    }
    let [main, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let gap_w = gutter::gutter_width(&app.rows);
    // Only the visible regions get a column: the left panel can be toggled off
    // (`app.show_left`), and zoom keeps one region — a diff pane (hiding the
    // other, the gutter and both rulers) or the left panel (hiding both panes).
    // Omitted regions collapse to zero-size rects.
    let zero = Rect::new(0, 0, 0, 0);
    let left_zoom = matches!(app.zoom, Some(Focus::Files) | Some(Focus::Revisions));
    let show_old = !left_zoom && app.zoom != Some(Focus::New);
    let show_new = !left_zoom && app.zoom != Some(Focus::Old);
    let show_gap = !left_zoom && app.zoom.is_none();

    let mut constraints: Vec<Constraint> = Vec::new();
    if app.show_left {
        constraints.push(if left_zoom {
            // Zoomed, the left panel takes the screen.
            Constraint::Min(0)
        } else {
            // Otherwise never let it eat more than half the screen.
            Constraint::Length(LEFT_W.min(main.width / 2))
        });
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
    let files_col = take(app.show_left);
    let old_ruler = take(show_old);
    let old = take(show_old);
    let gap = take(show_gap);
    let new = take(show_new);
    let new_ruler = take(show_new);

    // The left column is split: changed files on top, versions below, so the
    // diff stays visible while choosing a version.
    let (files_area, revisions_area) = if app.show_left {
        let body = main.height;
        let rev_h = (body * 2 / 5).max(7).min(body.saturating_sub(3).max(1));
        let [files_area, revisions_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(rev_h)]).areas(files_col);
        (files_area, Some(revisions_area))
    } else {
        (files_col, None)
    };

    let [files_head, files_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(files_area);
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

    let (files_style, versions_style, old_style, new_style) = match app.focus {
        Focus::Files => (
            theme::active_style(),
            theme::dim_style(),
            theme::dim_style(),
            theme::dim_style(),
        ),
        Focus::Revisions => (
            theme::dim_style(),
            theme::active_style(),
            theme::dim_style(),
            theme::dim_style(),
        ),
        Focus::Old => (
            theme::dim_style(),
            theme::dim_style(),
            theme::active_style(),
            theme::dim_style(),
        ),
        Focus::New => (
            theme::dim_style(),
            theme::dim_style(),
            theme::dim_style(),
            theme::active_style(),
        ),
    };

    // The header names the worktree (its directory) and the branch checked out
    // in it, so several worktrees of the same repository stay distinguishable.
    let repo_name = app
        .repo
        .as_ref()
        .and_then(|r| r.root().file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let repo_label = repo_label(&repo_name, &app.branch);
    // A revision comparison names itself next to the repo; the worktree view
    // (the default) has nothing extra to say.
    let range = match app.comparison {
        crate::git::Comparison::Worktree => None,
        ref other => Some(other.label()),
    };
    if app.show_left {
        let suffix = range
            .as_ref()
            .map(|label| format!("  {label}"))
            .unwrap_or_default();
        f.render_widget(
            Paragraph::new(format!(" {repo_label}{suffix}")).style(files_style),
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

    // A direct file comparison shows the paths; a revision comparison names the
    // two revs; the worktree view just says which side is which.
    let (old_title, new_title) = match (&app.pair, app.comparison.side_labels()) {
        (Some((old, new)), _) => (
            format!(" old  {}", old.display()),
            format!(" new  {}", new.display()),
        ),
        (None, Some((old, new))) => (format!(" old  {old}"), format!(" new  {new}")),
        (None, None) => (" old".to_string(), " new".to_string()),
    };
    f.render_widget(Paragraph::new(old_title).style(old_style), old_head);
    f.render_widget(
        Paragraph::new(panes::build_side(&app.rows, true, old_w, app.left(true)))
            .scroll((old_top as u16, 0)),
        old_body,
    );
    f.render_widget(Paragraph::new(new_title).style(new_style), new_head);
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

    // Symbol results sit on top of the panes until a jump or Esc.
    if app.locations_active() {
        draw_locations(f, app);
    }

    // The versions list shares the left column, under the changed files.
    if let Some(area) = revisions_area {
        draw_revisions(f, app, area, versions_style);
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
    let (block, block_fg) = if active {
        (theme::CURSOR_ACTIVE, theme::CURSOR_ACTIVE_FG)
    } else {
        (theme::CURSOR_INACTIVE, theme::CURSOR_INACTIVE_FG)
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
        // A dark-on-bright block so the letter under the cursor stays readable.
        buf[(x, y)].set_bg(block);
        buf[(x, y)].set_fg(block_fg);
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

/// The `gd`/`gr` result list, centred over the panes.
fn draw_locations(f: &mut Frame, app: &App) {
    let Some(list) = &app.locations else {
        return;
    };
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let w = (area.width.saturating_mul(4) / 5).max(20).min(area.width);
    let h = (area.height.saturating_mul(3) / 5).max(5).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    let title = format!(" {}  ({} matches) ", list.title, list.items.len());
    let block = Block::bordered()
        .title(title)
        .border_style(theme::dim_style());
    let inner = block.inner(rect);
    f.render_widget(Clear, rect);
    f.render_widget(block, rect);

    let visible = inner.height as usize;
    if visible == 0 {
        return;
    }
    let offset = list.selected.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line> = list
        .items
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(index, loc)| {
            let style = if index == list.selected {
                theme::active_style().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!(
                    " {}:{}  {}",
                    app.location_label(&loc.path),
                    loc.line + 1,
                    loc.text.trim()
                ),
                style,
            ))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// The versions list, docked under the changed files: one row per commit,
/// newest first, with the tag or branch in a fixed column. There is no frame —
/// a labelled horizontal rule separates it from the file list.
fn draw_revisions(f: &mut Frame, app: &App, area: Rect, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let hint = if app.revisions.is_empty() {
        "versions".to_string()
    } else {
        match app
            .revision_marked
            .and_then(|marked| app.revisions.get(marked))
            .and_then(|row| row.revision())
        {
            Some(base) => format!("versions: base {}", base.label()),
            None => "versions: Enter compares with working tree".to_string(),
        }
    };
    // The rule doubles as the panel's label; the rest of the line is filled.
    let width = area.width as usize;
    let label = format!("─ {hint} ");
    let rule: String = if label.chars().count() < width {
        let mut rule = label;
        rule.push_str(&"─".repeat(width - rule.chars().count()));
        rule
    } else {
        label.chars().take(width).collect()
    };
    f.render_widget(
        Paragraph::new(rule).style(style),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    let visible = body.height as usize;
    if visible == 0 || app.revisions.is_empty() {
        return;
    }
    let offset = app
        .revision_selected
        .saturating_sub(visible.saturating_sub(1));
    let graph_w = graph_width(app);
    let lines: Vec<Line> = app
        .revisions
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(index, row)| match row {
            // Graph-only rows connect the commits above and below; they are
            // never selected, so they get no marker.
            crate::git::VersionRow::Link(graph) => {
                Line::from(Span::styled(graph_cell(graph, graph_w), theme::dim_style()))
            }
            crate::git::VersionRow::Commit(rev) => {
                let marked = Some(index) == app.revision_marked;
                let marker = if marked { "▸" } else { " " };
                let mut style = if index == app.revision_selected {
                    theme::active_style().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                // The base keeps its own colour whether or not the list is
                // focused.
                if marked {
                    style = style
                        .bg(theme::BASE_BG)
                        .fg(theme::BASE_FG)
                        .add_modifier(Modifier::BOLD);
                }
                Line::from(Span::styled(
                    format!(
                        "{}{marker}{} {}",
                        graph_cell(&rev.graph, graph_w),
                        label_cell(&rev.label()),
                        rev.subject
                    ),
                    style,
                ))
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), body);
}

/// The graph column width: the widest art in the list, capped so the names and
/// subjects keep their room. Padding every row to it keeps the label and
/// subject columns aligned.
fn graph_width(app: &App) -> usize {
    app.revisions
        .iter()
        .map(|row| match row {
            crate::git::VersionRow::Commit(rev) => rev.graph.chars().count(),
            crate::git::VersionRow::Link(graph) => graph.chars().count(),
        })
        .max()
        .unwrap_or(0)
        .min(GRAPH_W)
}

/// One row's graph art in a `width`-wide cell. When the art is wider than the
/// column the leftmost lanes are dropped — the commit marker sits at the right
/// end, so the row still reads as a commit.
fn graph_cell(graph: &str, width: usize) -> String {
    let chars: Vec<char> = graph.chars().collect();
    if chars.len() > width {
        chars[chars.len() - width..].iter().collect()
    } else {
        let mut cell = graph.to_string();
        cell.push_str(&" ".repeat(width - chars.len()));
        cell
    }
}

/// Pad (or clip) the revision's label into a fixed column so the subjects line
/// up. The label is its tag, checked-out branch or other ref, else the hash.
fn label_cell(label: &str) -> String {
    const W: usize = 18;
    let mut cell: String = label.chars().take(W - 1).collect();
    if label.chars().count() >= W {
        cell.push('…');
    }
    let pad = W.saturating_sub(cell.chars().count());
    format!("{cell}{}", " ".repeat(pad))
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
        Paragraph::new(format!(
            " differ {VERSION} ({COMMIT}, {COMMIT_TIME})    keys    j/k scroll    ? / Esc close"
        ))
        .style(theme::dim_style()),
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
    // The revision prompt is the other command-line-style input. A rejected
    // spec leaves the prompt open, so its message is shown beside it.
    if let Some(input) = app.range_text() {
        let mut text = format!("revs: {input}");
        if !app.message.is_empty() {
            text.push_str(&format!("   {}", app.message));
        }
        let col = "revs: ".chars().count() + input.chars().count();
        return (text, Some(col as u16));
    }
    // which-key: while a prefix is pending, show just that prefix's options.
    if !app.key_hint.is_empty() {
        return (format!(" {}", app.key_hint), None);
    }
    if app.locations_active() {
        return (
            " symbol results    j/k move    Enter open    Esc close".to_string(),
            None,
        );
    }
    // Focused versions list: say what Enter and m do, since they are not
    // self-evident from the panel.
    let version_hint = if app.focus == Focus::Revisions {
        "Enter compare   m mark base   Space V type   "
    } else {
        ""
    };
    let path = match app.pair.as_ref() {
        Some((old, new)) => format!("{} → {}", old.display(), new.display()),
        None => app.selected_path().unwrap_or("(no file)").to_string(),
    };
    let cur = app.old_top.min(old_total);
    let pct = if old_total == 0 {
        100
    } else {
        cur * 100 / old_total
    };
    let focus = match app.focus {
        Focus::Files => "files",
        Focus::Revisions => "versions",
        Focus::Old => "old",
        Focus::New => "new",
    };
    // Keep messages near the front so they are visible.
    let msg = if app.message.is_empty() {
        String::new()
    } else {
        format!("{}   ", app.message)
    };
    let range = match app.comparison {
        crate::git::Comparison::Worktree => String::new(),
        ref other => format!("  [{}]", other.label()),
    };
    let text = format!(
        " {path}  {pct}% ({cur}/{old_total})  [{focus}]{range}   {version_hint}{msg}? help   q quit"
    );
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
        app.show_left = false;
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
        app.show_left = false;
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
        app.show_left = false;
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
    fn file_pair_paths_replace_the_side_labels() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.pair = Some(("old.txt".into(), "new.txt".into()));
        app.show_left = false;
        let mut term = Terminal::new(TestBackend::new(60, 4)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let top: String = (0..60).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(top.contains("old  old.txt"), "header: {top:?}");
        assert!(top.contains("new  new.txt"), "header: {top:?}");
    }

    #[test]
    fn the_repo_label_names_the_worktree_and_branch() {
        assert_eq!(repo_label("differ", "main"), "differ @ main");
        assert_eq!(repo_label("differ", ""), "differ");
        assert_eq!(repo_label("", "main"), "main");
    }

    #[test]
    fn the_files_header_names_the_current_branch() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.branch = "feature-x".into();
        let mut term = Terminal::new(TestBackend::new(60, 8)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let top: String = (0..60).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(top.contains("feature-x"), "header: {top:?}");
    }

    #[test]
    fn revision_comparisons_name_both_sides_and_the_status_line() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.show_left = false;
        app.comparison = crate::git::Comparison::Revs {
            from: crate::git::Rev {
                commit: "1a2b3c4d5e6".into(),
                label: "v0.2.0".into(),
            },
            to: crate::git::Rev {
                commit: "9f8e7d6c5b4".into(),
                label: "HEAD".into(),
            },
        };
        let mut term = Terminal::new(TestBackend::new(80, 4)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        let top: String = (0..80).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(top.contains("old  v0.2.0 (1a2b3c4)"), "header: {top:?}");
        assert!(top.contains("new  HEAD (9f8e7d6)"), "header: {top:?}");

        let (line, _) = help_line(&app, 1);
        assert!(line.contains("[v0.2.0..HEAD]"), "status: {line:?}");
    }

    #[test]
    fn the_revision_prompt_takes_the_help_line_with_a_caret() {
        use ratatui::crossterm::event::KeyCode;
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.start_range_prompt();
        app.range_key(KeyCode::Char('v'));
        let (text, caret) = help_line(&app, 1);
        assert_eq!(text, "revs: v");
        // The caret sits just past the typed spec ("revs: " is 6 columns).
        assert_eq!(caret, Some(7));
    }

    #[test]
    fn the_left_panel_stacks_changes_above_versions() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.revisions = vec![crate::git::VersionRow::Commit(crate::git::Revision {
            commit: "abc1234".into(),
            short: "abc1234".into(),
            refs: "tag: v1".into(),
            subject: "First commit".into(),
            graph: "* ".into(),
        })];
        app.revision_selected = 0;
        app.focus = Focus::Revisions;
        let mut term = Terminal::new(TestBackend::new(100, 16)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();

        // The diff header owns the top row to the right of the left column.
        let top: String = (0..100).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        let old_at = top.find("old").expect("old pane header");
        assert!(old_at >= 46, "panes start after the left column: {top:?}");

        // The versions panel sits in the left column, under the file list.
        let mut title = None;
        for y in 0..16u16 {
            for x in 0..91u16 {
                let word: String = (x..x + 8)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect();
                if word == "versions" {
                    title = Some((x, y));
                }
            }
        }
        let (x, y) = title.expect("versions title");
        assert!(x < 46, "inside the left column");
        assert!(y > 1, "below the file list");

        let text: String = (0..16)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol().to_string())
            .collect();
        assert!(
            text.contains("versions: Enter compares with working tree"),
            "{text}"
        );
        assert!(text.contains("v1"), "{text}");
        assert!(text.contains("First commit"), "{text}");

        // The keys that are not obvious from the panel are hinted while it has
        // focus, and only then.
        let (line, _) = help_line(&app, 1);
        assert!(line.contains("[versions]"), "status: {line:?}");
        assert!(line.contains("m mark base"), "status: {line:?}");
        app.focus = Focus::Old;
        let (line, _) = help_line(&app, 1);
        assert!(!line.contains("m mark base"), "status: {line:?}");
    }

    #[test]
    fn zoomed_left_panel_takes_the_whole_screen() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.revisions = vec![crate::git::VersionRow::Commit(crate::git::Revision {
            commit: "abc1234".into(),
            short: "abc1234".into(),
            refs: "tag: v1".into(),
            subject: "First commit".into(),
            graph: "* ".into(),
        })];
        app.focus = Focus::Files;
        app.toggle_zoom();
        let mut term = Terminal::new(TestBackend::new(80, 14)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();

        let top: String = (0..80).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(!top.contains("old"), "no diff headers when zoomed: {top:?}");
        // The versions rule spans the whole width, starting at the first column.
        let mut rule_y = None;
        for y in 0..14u16 {
            let row: String = (0..80).map(|x| buf[(x, y)].symbol().to_string()).collect();
            if row.contains("versions:") {
                rule_y = Some(y);
                assert_eq!(buf[(0, y)].symbol(), "─");
                assert_eq!(buf[(79, y)].symbol(), "─", "rule to the edge: {row:?}");
            }
        }
        assert!(rule_y.is_some(), "versions rule");
    }

    #[test]
    fn the_marked_base_has_its_own_colour_and_marker() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.revisions = vec![
            crate::git::VersionRow::Commit(crate::git::Revision {
                commit: "aaaaaaa".into(),
                short: "aaaaaaa".into(),
                refs: "HEAD -> main".into(),
                subject: "Tip".into(),
                graph: "* ".into(),
            }),
            crate::git::VersionRow::Commit(crate::git::Revision {
                commit: "bbbbbbb".into(),
                short: "bbbbbbb".into(),
                refs: "tag: v1".into(),
                subject: "Base".into(),
                graph: "* ".into(),
            }),
        ];
        app.revision_selected = 0;
        app.revision_marked = Some(1);
        app.focus = Focus::Revisions;
        let mut term = Terminal::new(TestBackend::new(100, 16)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();

        let (mut base_y, mut tip_y) = (None, None);
        for y in 0..16u16 {
            let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
            if row.contains("Base") {
                base_y = Some(y);
            }
            if row.contains("Tip") {
                tip_y = Some(y);
            }
        }
        let base_y = base_y.expect("base row");
        let tip_y = tip_y.expect("cursor row");

        // The base is marked and tinted, so it reads even without focus.
        // The row starts with the graph prefix ("* "), then the marker.
        assert_eq!(buf[(2, base_y)].symbol(), "▸");
        assert_eq!(buf[(0, base_y)].bg, theme::BASE_BG);
        // The cursor row is reversed instead, a different look entirely.
        assert!(buf[(0, tip_y)].modifier.contains(Modifier::REVERSED));
        assert_ne!(buf[(0, tip_y)].bg, theme::BASE_BG);
    }

    #[test]
    fn versions_rows_show_the_branch_graph() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        let commit = |commit: &str, subject: &str, graph: &str| {
            crate::git::VersionRow::Commit(crate::git::Revision {
                commit: commit.into(),
                short: commit.into(),
                refs: String::new(),
                subject: subject.into(),
                graph: graph.into(),
            })
        };
        app.revisions = vec![
            commit("aaaaaaa", "Tip", "* "),
            crate::git::VersionRow::Link("|\\  ".into()),
            commit("bbbbbbb", "Side", "| * "),
            crate::git::VersionRow::Link("|/  ".into()),
        ];
        app.focus = Focus::Revisions;
        let mut term = Terminal::new(TestBackend::new(100, 16)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer();

        let rows: Vec<String> = (0..16u16)
            .map(|y| (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let tip = rows.iter().position(|row| row.contains("Tip")).unwrap();
        let side = rows.iter().position(|row| row.contains("Side")).unwrap();
        assert!(rows[tip].starts_with("* "), "tip row: {:?}", rows[tip]);
        assert!(rows[side].starts_with("| * "), "side row: {:?}", rows[side]);
        // The connector lines sit between the commits they join.
        assert!(
            rows[tip + 1].starts_with("|\\"),
            "link: {:?}",
            rows[tip + 1]
        );
        assert!(
            rows[side + 1].starts_with("|/"),
            "link: {:?}",
            rows[side + 1]
        );
        // A commit row is selectable; the link rows are not.
        app.focus_revisions();
        assert_eq!(app.revision_selected, 0);
    }

    #[test]
    fn location_list_draws_over_the_panes() {
        let mut app = test_app(vec![DiffRow::Ctx("x".into())], 5);
        app.show_left = false;
        app.locations = Some(crate::app::LocationList {
            title: "References `x`".into(),
            items: vec![crate::symbol::Location {
                path: std::path::PathBuf::from("src/lib.rs"),
                line: 3,
                byte_col: 0,
                text: "    x();".into(),
            }],
            selected: 0,
        });
        let mut term = Terminal::new(TestBackend::new(100, 16)).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        let text: String = (0..16)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .map(|(x, y)| term.backend().buffer()[(x, y)].symbol().to_string())
            .collect();
        assert!(text.contains("References `x`"));
        assert!(text.contains("src/lib.rs:4"));
        assert!(text.contains("Enter open"));
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
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains(env!("DIFFER_GIT_HASH")));
        assert!(text.contains(env!("DIFFER_COMMIT_TIME")));

        // With help closed, the which-key hint owns the bottom line.
        app.help_open = false;
        app.key_hint = "Space: e toggle file panel".into();
        let (line, caret) = help_line(&app, 1);
        assert!(line.contains("toggle file panel"));
        assert_eq!(caret, None);
    }
}
