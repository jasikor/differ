//! The inter-pane gutter: rails, connectors, gap arrows.
//!
//! Ported from the mockup. Rails live *inside* the panels (as overlays); the
//! gutter grid holds only the stepping connector.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    Frame,
};

use crate::model::{DiffRow, Rows};
use crate::view::theme::{
    LineStyle, ViewOpts, ADD_BOT, ADD_TOP, GAP, GUTTER_ADDED, GUTTER_CHANGED, GUTTER_REMOVED,
};

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: None,
            bg: None,
            bold: false,
        }
    }
}

fn blank_cells(width: usize) -> Vec<Cell> {
    vec![Cell::default(); width]
}

fn glyph(ch: char, color: Color) -> Cell {
    Cell {
        ch,
        fg: Some(color),
        bold: true,
        ..Cell::default()
    }
}

/// Draw a stepped line in the gutter from (c0row, col 0) to (c3row, last col).
fn connector_edge(
    cells: &mut [Vec<Cell>],
    c0row: usize,
    c3row: usize,
    spine: usize,
    mark: Color,
    style: LineStyle,
) {
    let right = cells[c0row].len() - 1;
    if c0row == c3row {
        for cell in cells[c0row].iter_mut().take(right).skip(1) {
            *cell = glyph(style.h, mark);
        }
        return;
    }
    let (top, bottom) = (c0row.min(c3row), c0row.max(c3row));
    for row in cells.iter_mut().take(bottom + 1).skip(top) {
        row[spine] = glyph(style.v, mark);
    }
    if c3row < c0row {
        cells[c0row][spine] = glyph(style.ul, mark);
        cells[c3row][spine] = glyph(style.dr, mark);
    } else {
        cells[c0row][spine] = glyph(style.dl, mark);
        cells[c3row][spine] = glyph(style.ur, mark);
    }
    for cell in cells[c0row].iter_mut().take(spine).skip(1) {
        *cell = glyph(style.h, mark);
    }
    for cell in cells[c3row].iter_mut().take(right).skip(spine + 1) {
        *cell = glyph(style.h, mark);
    }
}

fn cells_to_line(cells: &[Cell]) -> Line<'static> {
    let spans: Vec<Span<'static>> = cells
        .iter()
        .map(|c| {
            let mut span = Span::styled(c.ch.to_string(), Style::default());
            if let Some(fg) = c.fg {
                span = span.fg(fg);
            }
            if let Some(bg) = c.bg {
                span = span.bg(bg);
            }
            if c.bold {
                span = span.add_modifier(Modifier::BOLD);
            }
            span
        })
        .collect();
    Line::from(spans)
}

/// Where a connector should meet a rail: clamp the arrow's row into the rail's
/// row range.
fn rail_attach_row(arrow_row: i32, lo: i32, hi: i32, height: i32) -> usize {
    let lo = lo.max(0);
    let hi = hi.min(height - 1).max(lo);
    arrow_row.clamp(lo, hi) as usize
}

fn leading_ws(text: &str) -> usize {
    text.chars().take_while(|c| *c == ' ').count()
}

fn new_side_texts(rows: &[DiffRow]) -> Vec<&str> {
    let mut out = Vec::new();
    for r in rows {
        match r {
            DiffRow::Ctx(t) | DiffRow::Ins(t) => out.push(t.as_str()),
            DiffRow::Chg(_, n) => out.push(n.as_str()),
            DiffRow::Del(_) => {}
        }
    }
    out
}

fn old_side_texts(rows: &[DiffRow]) -> Vec<&str> {
    let mut out = Vec::new();
    for r in rows {
        match r {
            DiffRow::Ctx(t) | DiffRow::Del(t) => out.push(t.as_str()),
            DiffRow::Chg(o, _) => out.push(o.as_str()),
            DiffRow::Ins(_) => {}
        }
    }
    out
}

fn last_non_space(text: &str) -> Option<usize> {
    text.char_indices()
        .rfind(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| text[..i].chars().count())
}

/// A cell painted into a panel after the normal render.
#[derive(Clone, Copy)]
pub struct Ov {
    left: bool,
    col: usize,
    row: i32,
    ch: char,
    color: Color,
}

fn push_run(
    ovs: &mut Vec<Ov>,
    left: bool,
    row: i32,
    cols: impl Iterator<Item = usize>,
    ch: char,
    color: Color,
) {
    for col in cols {
        ovs.push(Ov {
            left,
            col,
            row,
            ch,
            color,
        });
    }
}

/// A cell whose background is reset, trimming the faint line tint.
#[derive(Clone, Copy)]
pub struct Clear {
    left: bool,
    row: i32,
    col: usize,
}

fn push_clear(clears: &mut Vec<Clear>, left: bool, row: i32, cols: impl Iterator<Item = usize>) {
    for col in cols {
        clears.push(Clear { left, row, col });
    }
}

/// Build the gutter lines plus the panel overlays that must be painted on top.
pub fn build_gutter(
    rows: &[DiffRow],
    opts: &ViewOpts,
    old_w: usize,
    new_w: usize,
) -> (Vec<Line<'static>>, Vec<Ov>, Vec<Clear>) {
    let width = GAP as usize;
    let delta = opts.delta;
    let style = opts.line;
    let (end_top, end_bot) = (opts.end_top, opts.end_bot);
    let (rail_left, rail_right) = (opts.rail_left, opts.rail_right);
    let new_texts = new_side_texts(rows);
    let old_texts = old_side_texts(rows);
    // Old panel text is prefixed by its right-aligned line number + a space.
    let old_off = old_texts.len().max(1).to_string().len() + 1;
    let mut ovs: Vec<Ov> = Vec::new();
    let mut clears: Vec<Clear> = Vec::new();
    let mut cells: Vec<Vec<Cell>> = Vec::new();
    // Each block: its rows in the old view, and its rows in the new view.
    let mut blocks: Vec<(Rows, Rows)> = Vec::new();
    let mut right_idx = 0usize;
    let mut i = 0;
    while i < rows.len() {
        match &rows[i] {
            DiffRow::Ctx(_) => {
                cells.push(blank_cells(width));
                right_idx += 1;
                i += 1;
            }
            DiffRow::Ins(_) => {
                let ls = cells.len();
                let rs = right_idx;
                while i < rows.len() && matches!(rows[i], DiffRow::Ins(_)) {
                    right_idx += 1;
                    i += 1;
                }
                blocks.push(((ls, ls), (rs, right_idx)));
            }
            DiffRow::Del(_) => {
                let ls = cells.len();
                let boundary = right_idx;
                while i < rows.len() && matches!(rows[i], DiffRow::Del(_)) {
                    cells.push(blank_cells(width));
                    i += 1;
                }
                blocks.push(((ls, cells.len()), (boundary, boundary)));
            }
            DiffRow::Chg(_, _) => {
                let ls = cells.len();
                let rs = right_idx;
                while i < rows.len() && matches!(rows[i], DiffRow::Chg(_, _)) {
                    cells.push(blank_cells(width));
                    right_idx += 1;
                    i += 1;
                }
                blocks.push(((ls, cells.len()), (rs, right_idx)));
            }
        }
    }
    // The grid is one row per *old* line, but new-side rails/arrows are placed
    // at (new_index - delta). When the new side has more lines than the old
    // (e.g. additions), those rows can fall past the end — pad the grid so they
    // stay in range.
    let new_rows = (new_texts.len() as i32 - delta).max(0) as usize;
    while cells.len() < new_rows {
        cells.push(blank_cells(width));
    }
    let height = cells.len() as i32;
    for (lane, (left, (rs, re))) in blocks.into_iter().enumerate() {
        if height == 0 {
            continue;
        }
        let removed = rs == re;
        let added = left.0 == left.1;
        let color = if removed {
            GUTTER_REMOVED
        } else if added {
            GUTTER_ADDED
        } else {
            GUTTER_CHANGED
        };
        let rs2 = rs as i32 - delta;
        let re2 = re as i32 - delta;
        let c3 = rs2.clamp(0, height - 1) as usize;
        let spine = 1 + (lane % (width - 2));
        let (c0row, c3row) = if added {
            let rail_end = rail_attach_row(left.0 as i32, rs2, re2 - 1, height);
            (left.0.saturating_sub(1), rail_end)
        } else if removed {
            let rail_end = rail_attach_row(rs2, left.0 as i32, left.1 as i32 - 1, height);
            (rail_end, c3)
        } else {
            (
                rail_attach_row(rs2, left.0 as i32, left.1 as i32 - 1, height),
                rail_attach_row(left.0 as i32, rs2, re2 - 1, height),
            )
        };
        connector_edge(&mut cells, c0row, c3row, spine, color, style);
        cells[c0row][0] = glyph(style.h, color);
        cells[c3row][width - 1] = glyph(style.h, color);

        // ---- old (left) panel ----
        if !added {
            let end = (left.0..left.1)
                .filter_map(|r| old_texts.get(r))
                .filter_map(|t| last_non_space(t))
                .max();
            let lc = (old_off + end.map_or(0, |e| e + 1) + 1).min(old_w.saturating_sub(1));
            for row in (left.0 as i32)..(left.1 as i32) {
                ovs.push(Ov {
                    left: true,
                    col: lc,
                    row,
                    ch: rail_left,
                    color,
                });
            }
            ovs.push(Ov {
                left: true,
                col: lc,
                row: c0row as i32,
                ch: style.vr,
                color,
            });
            push_run(
                &mut ovs,
                true,
                c0row as i32,
                (lc + 1)..old_w,
                style.h,
                color,
            );
            for r in left.0..left.1 {
                let first_col = old_off + old_texts.get(r).map_or(0, |t| leading_ws(t));
                push_clear(&mut clears, true, r as i32, old_off..first_col);
                push_clear(&mut clears, true, r as i32, lc..old_w);
            }
        } else if (left.0 as i32) < height {
            if left.0 >= 1 {
                let end = old_texts
                    .get(left.0 - 1)
                    .and_then(|t| last_non_space(t))
                    .max(old_texts.get(left.0).and_then(|t| last_non_space(t)));
                let col = (old_off + end.map_or(0, |e| e + 1) + 1).min(old_w.saturating_sub(1));
                push_run(
                    &mut ovs,
                    true,
                    (left.0 - 1) as i32,
                    (col + 1)..old_w,
                    style.h,
                    color,
                );
                ovs.push(Ov {
                    left: true,
                    col,
                    row: (left.0 - 1) as i32,
                    ch: ADD_TOP,
                    color,
                });
                ovs.push(Ov {
                    left: true,
                    col,
                    row: left.0 as i32,
                    ch: ADD_BOT,
                    color,
                });
            } else {
                ovs.push(Ov {
                    left: true,
                    col: 0,
                    row: left.0 as i32,
                    ch: ADD_BOT,
                    color,
                });
            }
        }

        // ---- new (right) panel ----
        if !removed {
            let start = (rs..re)
                .filter_map(|r| new_texts.get(r))
                .map(|t| leading_ws(t))
                .min()
                .unwrap_or(0);
            // The rail hugs the start of the code. With >=2 columns of indent
            // there is room inside the panel; otherwise (top-level lines) fall
            // back to the gutter's right edge so we never overwrite the code.
            if start >= 2 && start - 2 < new_w {
                let rc = start - 2;
                for r in rs2..re2 {
                    if r >= 0 && r < height {
                        ovs.push(Ov {
                            left: false,
                            col: rc,
                            row: r,
                            ch: rail_right,
                            color,
                        });
                    }
                }
                ovs.push(Ov {
                    left: false,
                    col: rc,
                    row: c3row as i32,
                    ch: style.vl,
                    color,
                });
                push_run(&mut ovs, false, c3row as i32, 0..rc, style.h, color);
            } else {
                for r in rs2..re2 {
                    if r >= 0 && r < height {
                        cells[r as usize][width - 1] = glyph(rail_right, color);
                    }
                }
                cells[c3row][width - 1] = glyph(style.vl, color);
            }
            for (i, r) in (rs..re).enumerate() {
                let text = new_texts.get(r);
                let first = text.map_or(0, |t| leading_ws(t));
                let end = text
                    .and_then(|t| last_non_space(t))
                    .map_or(first, |e| e + 1);
                let row = rs2 + i as i32;
                push_clear(&mut clears, false, row, 0..first);
                push_clear(&mut clears, false, row, end..new_w);
            }
        } else if rs2 >= 0 && rs2 < height {
            let indent = new_texts.get(rs).map_or(0, |t| leading_ws(t));
            if indent >= 2 {
                let col = indent - 2;
                push_run(&mut ovs, false, rs2, 0..col, style.h, color);
                ovs.push(Ov {
                    left: false,
                    col,
                    row: rs2,
                    ch: end_bot,
                    color,
                });
                ovs.push(Ov {
                    left: false,
                    col,
                    row: rs2 - 1,
                    ch: end_top,
                    color,
                });
            } else {
                cells[rs2 as usize][width - 1] = glyph(end_bot, color);
                if rs2 >= 1 {
                    cells[(rs2 - 1) as usize][width - 1] = glyph(end_top, color);
                }
            }
        }
    }
    (
        cells.iter().map(|row| cells_to_line(row)).collect(),
        ovs,
        clears,
    )
}

fn paint(f: &mut Frame, x: u16, y: u16, ch: char, color: Color) {
    let cell = &mut f.buffer_mut()[(x, y)];
    cell.set_char(ch);
    cell.set_fg(color);
}

/// Paint the rails, gap arrows and bridges into the panels, then reset the
/// background outside [first letter .. rail].
pub fn draw_overlays(
    f: &mut Frame,
    ovs: &[Ov],
    clears: &[Clear],
    old_body: Rect,
    new_body: Rect,
    scroll: u16,
) {
    let row_of = |body: Rect, row: i32| -> Option<u16> {
        let top = body.y as i32;
        let bottom = (body.y + body.height) as i32;
        let y = top + row - scroll as i32;
        (y >= top && y < bottom).then_some(y as u16)
    };
    for ov in ovs {
        let body = if ov.left { old_body } else { new_body };
        if body.width == 0 || ov.col >= body.width as usize {
            continue;
        }
        if let Some(y) = row_of(body, ov.row) {
            paint(f, body.x + ov.col as u16, y, ov.ch, ov.color);
        }
    }
    for c in clears {
        let body = if c.left { old_body } else { new_body };
        if body.width == 0 || c.col >= body.width as usize {
            continue;
        }
        if let Some(y) = row_of(body, c.row) {
            f.buffer_mut()[(body.x + c.col as u16, y)].set_bg(Color::Reset);
        }
    }
}
