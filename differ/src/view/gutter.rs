//! The inter-pane gutter: rails, connectors, gap arrows.
//!
//! Ported from the mockup. Rails live *inside* the panels (as overlays); the
//! gutter grid holds only the stepping connector.

use ratatui::{
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span},
    Frame,
};

use crate::model::{DiffRow, Rows};
use crate::view::theme::{
    LineStyle, ADD_BOT, ADD_BOT_FLIP, ADD_TOP, ADD_TOP_FLIP, END_BOT, END_BOT_FLIP, END_TOP,
    END_TOP_FLIP, GAP, LINE, MIN_GAP, NEW_MARGIN, RAIL, RAIL_ADDED, RAIL_CHANGED, RAIL_REMOVED,
};

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
    /// Connector directions (bitmask of UP/DOWN/LEFT/RIGHT); 0 means the cell is
    /// not part of a connector and `ch` is used as-is.
    dirs: u8,
    /// Set when connectors of different colours meet here (a crossing).
    mixed: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: None,
            bg: None,
            dirs: 0,
            mixed: false,
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
        ..Cell::default()
    }
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// Neutral colour for where two differently-coloured connectors cross, so
/// neither line appears to bleed into the other.
const CROSS: Color = Color::Rgb(0x6b, 0x72, 0x7b);

/// OR `mask` into a cell's connector directions. The first colour wins; if a
/// different colour also passes through, the cell is marked as a crossing.
fn link(cells: &mut [Vec<Cell>], row: usize, col: usize, mask: u8, color: Color) {
    if let Some(cell) = cells.get_mut(row).and_then(|r| r.get_mut(col)) {
        cell.dirs |= mask;
        match cell.fg {
            None => cell.fg = Some(color),
            Some(c) if c != color => cell.mixed = true,
            _ => {}
        }
    }
}

/// The glyph for a set of connector directions.
fn mask_glyph(mask: u8, style: LineStyle) -> char {
    match mask {
        m if m == UP | DOWN => style.v,
        m if m == LEFT | RIGHT => style.h,
        m if m == UP | LEFT => style.ul,
        m if m == UP | RIGHT => style.ur,
        m if m == DOWN | RIGHT => style.dr,
        m if m == DOWN | LEFT => style.dl,
        m if m == UP | DOWN | RIGHT => style.vr,
        m if m == UP | DOWN | LEFT => style.vl,
        m if m == LEFT | RIGHT | UP => '┴',
        m if m == LEFT | RIGHT | DOWN => '┬',
        m if m == UP | DOWN | LEFT | RIGHT => '┼',
        _ => style.h,
    }
}

/// Draw a stepped connector from (c0row, col 0) to (c3row, last col). Segments
/// are accumulated as direction bits so crossings render as junctions instead
/// of one connector chopping through another.
fn connector_edge(cells: &mut [Vec<Cell>], c0row: usize, c3row: usize, spine: usize, mark: Color) {
    let right = cells[0].len() - 1;
    if c0row == c3row {
        for col in 1..right {
            link(cells, c0row, col, LEFT | RIGHT, mark);
        }
        return;
    }
    let (top, bottom) = (c0row.min(c3row), c0row.max(c3row));
    for row in (top + 1)..bottom {
        link(cells, row, spine, UP | DOWN, mark);
    }
    if c3row < c0row {
        link(cells, c0row, spine, UP | LEFT, mark);
        link(cells, c3row, spine, DOWN | RIGHT, mark);
    } else {
        link(cells, c0row, spine, DOWN | LEFT, mark);
        link(cells, c3row, spine, UP | RIGHT, mark);
    }
    for col in 1..spine {
        link(cells, c0row, col, LEFT | RIGHT, mark);
    }
    for col in (spine + 1)..right {
        link(cells, c3row, col, LEFT | RIGHT, mark);
    }
}

/// The two rows a block's connector joins: the old-side attach (`c0row`) and the
/// new-side attach (`c3row`).
#[allow(clippy::too_many_arguments)]
fn connector_rows(
    left: Rows,
    rs: usize,
    re: usize,
    added: bool,
    removed: bool,
    flip: bool,
    pair_shift: i32,
    height: i32,
) -> (usize, usize) {
    let rs2 = rs as i32 + pair_shift;
    let re2 = re as i32 + pair_shift;
    if added {
        let rail_end = rail_attach_row(left.0 as i32, rs2, re2 - 1, height);
        let attach = if flip {
            left.0 as i32 - 1
        } else {
            left.0 as i32
        };
        (attach.max(0) as usize, rail_end)
    } else if removed {
        let rail_end = rail_attach_row(rs2, left.0 as i32, left.1 as i32 - 1, height);
        let attach = if flip { rs2 - 1 } else { rs2 };
        (rail_end, attach.clamp(0, height - 1) as usize)
    } else {
        (
            rail_attach_row(rs2, left.0 as i32, left.1 as i32 - 1, height),
            rail_attach_row(left.0 as i32, rs2, re2 - 1, height),
        )
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

/// The gutter width a diff needs: one lane per connector block (each block gets
/// its own lane, so connectors never have to share one), plus the two edge
/// columns. Small diffs keep a narrow gutter so the panes stay close together.
pub fn gutter_width(rows: &[DiffRow]) -> u16 {
    let mut blocks = 0u16;
    let mut prev: Option<u8> = None;
    for r in rows {
        let kind = match r {
            DiffRow::Ctx(_) => None,
            DiffRow::Ins(_) => Some(0),
            DiffRow::Del(_) => Some(1),
            DiffRow::Chg(_, _) => Some(2),
        };
        if let Some(k) = kind {
            if prev != Some(k) {
                blocks += 1;
            }
        }
        prev = kind;
    }
    (blocks + 2).clamp(MIN_GAP, GAP)
}

/// Build the gutter lines plus the panel overlays that must be painted on top.
///
/// `pair_shift` = `old_top - new_top`: it is how far the new-side rows are
/// offset relative to the old-view gutter grid, so a rail/arrow lands beside
/// the matching new line.
pub fn build_gutter(
    rows: &[DiffRow],
    pair_shift: i32,
    gap: u16,
    old_w: usize,
    new_w: usize,
) -> (Vec<Line<'static>>, Vec<Ov>, Vec<Clear>) {
    let width = gap as usize;
    let style = LINE;
    let (end_top, end_bot) = (END_TOP, END_BOT);
    let new_texts = new_side_texts(rows);
    let old_texts = old_side_texts(rows);
    // Old panel text is prefixed by its right-aligned line number + a space.
    let old_off = old_texts.len().max(1).to_string().len() + 1;
    let mut ovs: Vec<Ov> = Vec::new();
    let mut clears: Vec<Clear> = Vec::new();
    let mut cells: Vec<Vec<Cell>> = Vec::new();
    // Each block: its rows in the old view, and its rows in the new view.
    let mut blocks: Vec<(Rows, Rows)> = Vec::new();
    // Whether each block starts a new hunk (a context line separates them).
    let mut new_hunk: Vec<bool> = Vec::new();
    let mut hunk_start = true;
    let mut right_idx = 0usize;
    let mut i = 0;
    while i < rows.len() {
        match &rows[i] {
            DiffRow::Ctx(_) => {
                cells.push(blank_cells(width));
                right_idx += 1;
                hunk_start = true;
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
                new_hunk.push(hunk_start);
                hunk_start = false;
            }
            DiffRow::Del(_) => {
                let ls = cells.len();
                let boundary = right_idx;
                while i < rows.len() && matches!(rows[i], DiffRow::Del(_)) {
                    cells.push(blank_cells(width));
                    i += 1;
                }
                blocks.push(((ls, cells.len()), (boundary, boundary)));
                new_hunk.push(hunk_start);
                hunk_start = false;
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
                new_hunk.push(hunk_start);
                hunk_start = false;
            }
        }
    }
    // The grid is one row per *old* line, but new-side rails/arrows are placed
    // at (new_index + pair_shift). When that runs past the end, pad the grid so
    // they stay in range.
    let new_rows = (new_texts.len() as i32 + pair_shift).max(0) as usize;
    while cells.len() < new_rows {
        cells.push(blank_cells(width));
    }
    let height = cells.len() as i32;

    // Connector endpoints, so lanes can be ordered to avoid crossings. Within a
    // hunk (blocks are monotone top-to-bottom) assigning lanes in the travel
    // direction's natural order keeps them disjoint. The direction is per hunk,
    // and each hunk uses its own lane numbering.
    let lanes = (width - 2).max(1);
    let mut conns: Vec<(usize, usize)> = Vec::with_capacity(blocks.len());
    for &(left, (rs, re)) in &blocks {
        let added = left.0 == left.1;
        let removed = rs == re;
        let rs2 = rs as i32 + pair_shift;
        let re2 = re as i32 + pair_shift;
        let flip = if removed {
            rs2 > left.0 as i32
        } else if added {
            re2 <= left.0 as i32
        } else {
            false
        };
        conns.push(connector_rows(
            left, rs, re, added, removed, flip, pair_shift, height,
        ));
    }
    let mut spines = vec![1usize; blocks.len()];
    let mut first = 0;
    while first < blocks.len() {
        let mut end = first + 1;
        while end < blocks.len() && !new_hunk[end] {
            end += 1;
        }
        let ups = (first..end).filter(|&k| conns[k].1 < conns[k].0).count();
        let up = ups * 2 > end - first;
        for (offset, spine) in spines[first..end].iter_mut().enumerate() {
            let idx = (first + offset) % lanes;
            *spine = if up { 1 + idx } else { lanes - idx };
        }
        first = end;
    }

    // An old block's rail shares its column with a neighbouring added block's
    // two-row gap arrow when they are adjacent (the arrow sits on the old
    // block's last row). Nudge the rail one column right there so both stay
    // visible instead of the arrow painting over the rail.
    let mut rail_off = vec![0usize; blocks.len()];
    for k in 0..blocks.len() {
        let (left, _) = blocks[k];
        if left.0 == left.1 {
            continue; // added block: no old-side rail
        }
        for &(l2, _) in &blocks {
            if l2.0 != l2.1 {
                continue; // not an added block
            }
            let (top, bot) = (l2.0 as i32 - 1, l2.0 as i32);
            let touches = |r: i32| r >= left.0 as i32 && r < left.1 as i32;
            if touches(top) || touches(bot) {
                rail_off[k] = 1;
                break;
            }
        }
    }

    // Mirror of `rail_off` for the new side: a removed block's gap arrow (drawn
    // on the new side) can share a column with a neighbour's new rail. Nudge
    // that rail one column right so both stay visible.
    let mut new_rail_off = vec![0usize; blocks.len()];
    for k in 0..blocks.len() {
        let (_, (rs, re)) = blocks[k];
        if rs == re {
            continue; // removed block: it has no new rail
        }
        let rc = (rs..re)
            .filter_map(|r| new_texts.get(r))
            .map(|t| leading_ws(t))
            .min()
            .unwrap_or(0);
        for &(_, (rs2, re2)) in &blocks {
            if rs2 != re2 {
                continue; // not a removed block
            }
            let indent = new_texts.get(rs2).map_or(0, |t| leading_ws(t));
            if indent < 2 {
                continue; // arrow lands in the gutter, no in-pane collision
            }
            let col = indent - 2;
            let (top, bot) = (rs2 as i32 + pair_shift - 1, rs2 as i32 + pair_shift);
            let (lo, hi) = (rs as i32 + pair_shift, re as i32 + pair_shift);
            if col == rc && top < hi && bot >= lo {
                new_rail_off[k] = 1;
                break;
            }
        }
    }

    for (lane, (left, (rs, re))) in blocks.into_iter().enumerate() {
        if height == 0 {
            continue;
        }
        let removed = rs == re;
        let added = left.0 == left.1;
        let color = if removed {
            RAIL_REMOVED
        } else if added {
            RAIL_ADDED
        } else {
            RAIL_CHANGED
        };
        let rs2 = rs as i32 + pair_shift;
        let re2 = re as i32 + pair_shift;
        // The two-row arrow points at the lines it marks. They sit at or below
        // the gap while the view is above them, so the tall triangle goes on the
        // bottom and the bridge on that row. Once the view has scrolled past the
        // block it sits above the gap, and the arrow flips (tall triangle up,
        // bridge on the top row) using the alternate glyph pair.
        let flip = if removed {
            rs2 > left.0 as i32
        } else if added {
            re2 <= left.0 as i32
        } else {
            false
        };
        // Lane order comes from the pre-pass (per-hunk, by travel direction).
        let spine = spines[lane];
        let (c0row, c3row) = conns[lane];
        connector_edge(&mut cells, c0row, c3row, spine, color);
        link(&mut cells, c0row, 0, LEFT | RIGHT, color);
        link(&mut cells, c3row, width - 1, LEFT | RIGHT, color);

        // ---- old (left) panel ----
        if !added {
            let end = (left.0..left.1)
                .filter_map(|r| old_texts.get(r))
                .filter_map(|t| last_non_space(t))
                .max();
            let raw = old_off + end.map_or(0, |e| e + 1) + 1;
            let lc = if rail_off[lane] == 1 {
                if raw >= old_w.saturating_sub(1) {
                    // Truncated: the arrow is pinned to the pane edge, so the
                    // rail steps one column left instead (nudging right would
                    // just clamp back onto the arrow).
                    old_w.saturating_sub(2)
                } else {
                    (raw + 1).min(old_w.saturating_sub(1))
                }
            } else {
                raw.min(old_w.saturating_sub(1))
            };
            for row in (left.0 as i32)..(left.1 as i32) {
                ovs.push(Ov {
                    left: true,
                    col: lc,
                    row,
                    ch: RAIL,
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
            // Rectangular block: from the block's minimum indent (ignoring
            // blank lines) out to the rail, on every row.
            let min_first = (left.0..left.1)
                .filter_map(|r| old_texts.get(r))
                .filter(|t| t.chars().any(|c| !c.is_whitespace()))
                .map(|t| leading_ws(t))
                .min()
                .unwrap_or(0);
            let first_col = old_off + min_first;
            for r in left.0..left.1 {
                push_clear(&mut clears, true, r as i32, old_off..first_col);
                push_clear(&mut clears, true, r as i32, lc..old_w);
            }
        } else if (left.0 as i32) < height {
            let (top_glyph, bot_glyph) = if flip {
                (ADD_TOP_FLIP, ADD_BOT_FLIP)
            } else {
                (ADD_TOP, ADD_BOT)
            };
            if left.0 >= 1 {
                let end = old_texts
                    .get(left.0 - 1)
                    .and_then(|t| last_non_space(t))
                    .max(old_texts.get(left.0).and_then(|t| last_non_space(t)));
                let col = (old_off + end.map_or(0, |e| e + 1) + 1).min(old_w.saturating_sub(1));
                // The bridge runs along the tall triangle's row: the bottom row
                // by default, the top row once the arrow has flipped.
                let bridge_row = if flip {
                    (left.0 - 1) as i32
                } else {
                    left.0 as i32
                };
                push_run(&mut ovs, true, bridge_row, (col + 1)..old_w, style.h, color);
                ovs.push(Ov {
                    left: true,
                    col,
                    row: (left.0 - 1) as i32,
                    ch: top_glyph,
                    color,
                });
                ovs.push(Ov {
                    left: true,
                    col,
                    row: left.0 as i32,
                    ch: bot_glyph,
                    color,
                });
            } else {
                ovs.push(Ov {
                    left: true,
                    col: 0,
                    row: left.0 as i32,
                    ch: bot_glyph,
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
            // The new pane indents its code by `NEW_MARGIN` (rail + space), so
            // the rail sits at `start` and the code begins two columns later.
            let rc = (start + new_rail_off[lane]).min(new_w.saturating_sub(1));
            for r in rs2..re2 {
                if r >= 0 && r < height {
                    ovs.push(Ov {
                        left: false,
                        col: rc,
                        row: r,
                        ch: RAIL,
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
            // A rectangular block: from the block's first column to its
            // longest line's end, on every row (not a ragged per-line run).
            // Blank lines don't count towards the left edge.
            let min_first = (rs..re)
                .filter_map(|r| new_texts.get(r))
                .filter(|t| t.chars().any(|c| !c.is_whitespace()))
                .map(|t| NEW_MARGIN + leading_ws(t))
                .min()
                .unwrap_or(0);
            let max_end = (rs..re)
                .filter_map(|r| new_texts.get(r))
                .filter_map(|t| last_non_space(t))
                .map(|e| NEW_MARGIN + e + 1)
                .max()
                .unwrap_or(0);
            for i in 0..(re - rs) {
                let row = rs2 + i as i32;
                push_clear(&mut clears, false, row, 0..min_first);
                push_clear(&mut clears, false, row, max_end..new_w);
            }
        } else if rs2 >= 0 && rs2 < height {
            let indent = new_texts.get(rs).map_or(0, |t| leading_ws(t));
            // Tall triangle on the gap-facing side, short on the far side; the
            // bridge always runs along the tall triangle's row. The flipped
            // orientation uses its own glyph pair.
            let (tall_glyph, short_glyph) = if flip {
                (END_TOP_FLIP, END_BOT_FLIP)
            } else {
                (end_bot, end_top)
            };
            let (tall_row, short_row) = if flip { (rs2 - 1, rs2) } else { (rs2, rs2 - 1) };
            if indent >= 2 {
                let col = indent - 2;
                push_run(&mut ovs, false, tall_row, 0..col, style.h, color);
                ovs.push(Ov {
                    left: false,
                    col,
                    row: tall_row,
                    ch: tall_glyph,
                    color,
                });
                ovs.push(Ov {
                    left: false,
                    col,
                    row: short_row,
                    ch: short_glyph,
                    color,
                });
            } else {
                if tall_row >= 0 && tall_row < height {
                    cells[tall_row as usize][width - 1] = glyph(tall_glyph, color);
                }
                if short_row >= 0 && short_row < height {
                    cells[short_row as usize][width - 1] = glyph(short_glyph, color);
                }
            }
        }
    }
    for row in cells.iter_mut() {
        for cell in row.iter_mut() {
            if cell.dirs != 0 {
                cell.ch = mask_glyph(cell.dirs, style);
                if cell.mixed {
                    cell.fg = Some(CROSS);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DiffRow;

    /// Tinted cells on the old side (code area only), for a gutter row.
    fn old_tint(clears: &[Clear], row: i32, old_off: usize, width: usize) -> Vec<usize> {
        (old_off..width)
            .filter(|&c| {
                !clears
                    .iter()
                    .any(|cl| cl.left && cl.row == row && cl.col == c)
            })
            .collect()
    }

    #[test]
    fn connectors_of_different_kinds_never_cross() {
        // A hunk mixing inserts and changes (like a block whose body grew), so
        // several connectors fan out between the same two rows. They must be
        // routed in separate lanes and never meet (no CROSS-coloured cells).
        let rep =
            |n: usize, f: &dyn Fn(usize) -> DiffRow| -> Vec<DiffRow> { (0..n).map(f).collect() };
        let mut rows = Vec::new();
        rows.extend(rep(7, &|i| DiffRow::Ctx(format!("h{i}"))));
        rows.extend(rep(15, &|i| DiffRow::Ins(format!("ins{i}"))));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("a{i}"))));
        rows.push(DiffRow::Ins("w".into()));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("b{i}"))));
        rows.push(DiffRow::Chg("c".into(), "C".into()));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("d{i}"))));
        rows.push(DiffRow::Chg("e".into(), "E".into()));
        rows.push(DiffRow::Chg("f".into(), "F".into()));
        rows.extend(rep(3, &|i| DiffRow::Ctx(format!("g{i}"))));
        rows.extend(rep(6, &|i| DiffRow::Ins(format!("tail{i}"))));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("z{i}"))));

        let (lines, _ovs, _clears) = build_gutter(&rows, 0, gutter_width(&rows), 40, 40);
        for (r, line) in lines.iter().enumerate() {
            for span in &line.spans {
                assert_ne!(span.style.fg, Some(CROSS), "crossing on gutter row {r}");
            }
        }
    }

    #[test]
    fn connectors_never_cross_while_scrolling() {
        let rep =
            |n: usize, f: &dyn Fn(usize) -> DiffRow| -> Vec<DiffRow> { (0..n).map(f).collect() };
        let mut rows = Vec::new();
        rows.extend(rep(7, &|i| DiffRow::Ctx(format!("h{i}"))));
        rows.extend(rep(15, &|i| DiffRow::Ins(format!("ins{i}"))));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("a{i}"))));
        rows.push(DiffRow::Ins("w".into()));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("b{i}"))));
        rows.push(DiffRow::Chg("c".into(), "C".into()));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("d{i}"))));
        rows.push(DiffRow::Chg("e".into(), "E".into()));
        rows.push(DiffRow::Chg("f".into(), "F".into()));
        rows.extend(rep(3, &|i| DiffRow::Ctx(format!("g{i}"))));
        rows.extend(rep(6, &|i| DiffRow::Ins(format!("tail{i}"))));
        rows.extend(rep(5, &|i| DiffRow::Ctx(format!("z{i}"))));

        for shift in -22i32..=22 {
            let (lines, _ovs, _clears) = build_gutter(&rows, shift, gutter_width(&rows), 38, 38);
            for (r, line) in lines.iter().enumerate() {
                for span in &line.spans {
                    assert_ne!(
                        span.style.fg,
                        Some(CROSS),
                        "crossing on gutter row {r} at shift {shift}"
                    );
                }
            }
        }
    }

    #[test]
    fn add_arrow_does_not_cover_an_adjacent_rail() {
        // A changed line immediately followed by an inserted line: the added
        // block's gap arrow (old side) would otherwise land on the Chg block's
        // rail. They must sit on different columns.
        let rows = vec![
            DiffRow::Ctx("fn main() {".into()),
            DiffRow::Chg(
                "    println!(\"sandbox\");".into(),
                "    println!(\"sandbox: hello\");".into(),
            ),
            DiffRow::Ins("    let x = 1;".into()),
            DiffRow::Ctx("}".into()),
        ];
        let (_, ovs, _) = build_gutter(&rows, 0, gutter_width(&rows), 40, 40);
        let rail = ovs.iter().find(|o| o.left && o.ch == RAIL).map(|o| o.col);
        let arrow = ovs
            .iter()
            .find(|o| o.left && (o.ch == ADD_TOP || o.ch == ADD_BOT))
            .map(|o| o.col);
        assert!(rail.is_some(), "expected an old-side rail");
        assert!(arrow.is_some(), "expected an old-side add arrow");
        assert_ne!(rail, arrow, "arrow would cover the rail");
    }

    #[test]
    fn add_arrow_and_rail_stay_apart_when_truncated() {
        // Narrow pane: the old line is truncated, so the rail clamps to the
        // pane edge, where the added block's arrow also lands.
        let rows = vec![
            DiffRow::Ctx("fn f() {".into()),
            DiffRow::Chg(
                "    let value = a_really_long_expression_here;".into(),
                "    let value = 1;".into(),
            ),
            DiffRow::Ins("    let extra = 2;".into()),
            DiffRow::Ctx("}".into()),
        ];
        let (_, ovs, _) = build_gutter(&rows, 0, gutter_width(&rows), 30, 30);
        let rail = ovs.iter().find(|o| o.left && o.ch == RAIL).map(|o| o.col);
        let arrow = ovs
            .iter()
            .find(|o| o.left && (o.ch == ADD_TOP || o.ch == ADD_BOT))
            .map(|o| o.col);
        assert!(rail.is_some(), "expected an old-side rail");
        assert!(arrow.is_some(), "expected an old-side add arrow");
        assert_ne!(rail, arrow, "truncated: arrow would cover the rail");
    }

    #[test]
    fn removed_arrow_does_not_cover_an_adjacent_new_rail() {
        // A removed line immediately followed by a changed line: the removed
        // block's gap arrow (new side) would otherwise land on the Chg block's
        // new rail.
        let rows = vec![
            DiffRow::Ctx("fn main() {".into()),
            DiffRow::Del("    let gone = 1;".into()),
            DiffRow::Chg("    let old = 2;".into(), "    let new = 3;".into()),
            DiffRow::Ctx("}".into()),
        ];
        let (_, ovs, _) = build_gutter(&rows, 0, gutter_width(&rows), 40, 40);
        let arrow = ovs
            .iter()
            .find(|o| !o.left && (o.ch == END_TOP || o.ch == END_BOT))
            .map(|o| o.col);
        let rails: Vec<usize> = ovs
            .iter()
            .filter(|o| !o.left && o.ch == RAIL)
            .map(|o| o.col)
            .collect();
        assert!(arrow.is_some(), "expected a removed arrow on the new side");
        assert!(!rails.is_empty(), "expected a new-side rail");
        assert!(
            !rails.contains(&arrow.unwrap()),
            "removed arrow would cover the new rail"
        );
    }

    #[test]
    fn removed_block_is_rectangular() {
        // A 2-line removed block indented by 4; a blank line must not pull the
        // left edge to 0.
        let rows = vec![
            DiffRow::Ctx("a".into()),
            DiffRow::Del("    removed one".into()),
            DiffRow::Del("    removed two".into()),
            DiffRow::Del("".into()),
            DiffRow::Ctx("b".into()),
        ];
        let (_lines, _ovs, clears) = build_gutter(&rows, 0, gutter_width(&rows), 40, 40);

        // old_off = 2 (1-digit line numbers + space); indent 4 -> first = 6;
        // rail = old_off + max_end(15) + 2 = 19.
        for row in [1, 2, 3] {
            let tint = old_tint(&clears, row, 2, 40);
            assert_eq!(tint.first(), Some(&6), "row {row} left edge");
            assert_eq!(tint.last(), Some(&17), "row {row} right edge");
        }
    }
}
