use std::{fs, io};

use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use similar::{ChangeTag, TextDiff};

const OLD_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/before.txt");
const NEW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/after.txt");

fn load_rows() -> io::Result<Vec<DiffRow>> {
    let old = fs::read_to_string(OLD_PATH)?;
    let new = fs::read_to_string(NEW_PATH)?;
    Ok(diff_rows(&old, &new))
}

// TODO(future): make the inter-panel gutter width dynamic — size it to fit the
// number of connector lanes currently in view — instead of this fixed value.
const GAP: u16 = 10;

// Backgrounds: faint tints of the line / row kind.
const REMOVED_LINE: Color = Color::Rgb(0x2e, 0x22, 0x22);
const ADDED_LINE: Color = Color::Rgb(0x1e, 0x2a, 0x24);
const MODIFIED_LINE: Color = Color::Rgb(0x1e, 0x1e, 0x1e);

// Span backgrounds / foregrounds: removed=red, added=green, changed=blue.
const REMOVED_BG: Color = Color::Rgb(0x4a, 0x2a, 0x2a);
const ADDED_BG: Color = Color::Rgb(0x29, 0x47, 0x34);
const CHANGED_BG: Color = Color::Rgb(0x26, 0x40, 0x5c);
const REMOVED_FG: Color = Color::Rgb(0xe0, 0x6c, 0x75);
const ADDED_FG: Color = Color::Rgb(0x98, 0xc3, 0x79);
const CHANGED_FG: Color = Color::Rgb(0x6c, 0xb6, 0xff);

// Gutter rails / arrows / connector lines: a muted version of the span
// foregrounds so they read clearly without overpowering the code.
const GUTTER_REMOVED: Color = Color::Rgb(0xa8, 0x51, 0x58);
const GUTTER_ADDED: Color = Color::Rgb(0x72, 0x92, 0x5b);
const GUTTER_CHANGED: Color = Color::Rgb(0x51, 0x89, 0xbf);

const HELP_MAIN: &str = " j/k scroll  PgUp/PgDn page  , . offset  R reload";

fn help_style() -> Style {
    Style::new().fg(Color::Rgb(0x8c, 0x96, 0xa0))
}

// NOTE: U+2500–U+25A0 (Box Drawing + Block Elements) are full of good
// candidates for line / rail / connector glyphs — worth browsing when
// choosing characters for the rails and connectors.
// NOTE: U+1CD00–U+1CDF4 (Symbols for Legacy Computing Supplement) is another
// rich source of line / rail / connector glyph candidates.
// End markers for the removed-line connector, to avoid a mid/top gap.
// End-marker pairs for removed blocks. The two glyphs sit on the lines that
// bracket where the removed lines used to be (top glyph on the line above the
// gap, bottom glyph on the line below), forming a small arrow that marks the
const END_PAIRS: [(&str, char, char); 9] = [
    ("U+1FB3D/U+1FB5A arrow", '\u{1FB3D}', '\u{1FB5A}'),
    ("U+1FB3D/U+1FB58 arrow", '\u{1FB3D}', '\u{1FB58}'),
    ("U+1FB3F/U+1FB5A arrow", '\u{1FB3F}', '\u{1FB5A}'),
    ("U+E0BE/U+E0BC arrow", '\u{E0BE}', '\u{E0BC}'),
    ("U+E0BA/U+E0B8 triangle", '\u{E0BA}', '\u{E0B8}'),
    ("▀/▄ half blocks", '▀', '▄'),
    ("▶/◀ arrows", '▶', '◀'),
    ("▸/◂ arrows", '▸', '◂'),
    ("╸/╴ half lines", '╸', '╴'),
];

// Mirrored arrow pair for the added-lines gap, which sits on the before side.
const ADD_TOP: char = '\u{1FB48}';
const ADD_BOT: char = '\u{1FB65}';

// Rail glyphs. U+258x partial blocks are the classic rails.
const RAILS: [char; 10] = ['▏', '▎', '▍', '▌', '▐', '▕', '█', '┃', '║', '│'];
const RAIL_NAMES: [&str; 10] = [
    "▏ one-eighth",
    "▎ one-quarter",
    "▍ three-eighths",
    "▌ half",
    "▐ right-half",
    "▕ right-eighth",
    "█ full",
    "┃ heavy",
    "║ double",
    "│ light",
];

// Connector line styles. Chars: horizontal, vertical, and the
// four corners (up-left, down-right, down-left, up-right).
#[derive(Clone, Copy)]
struct LineStyle {
    h: char,
    v: char,
    vr: char,
    vl: char,
    ul: char,
    dr: char,
    dl: char,
    ur: char,
}

const LINES: [(&str, LineStyle); 4] = [
    (
        "─│ light",
        LineStyle {
            h: '─',
            v: '│',
            vr: '├',
            vl: '┤',
            ul: '┘',
            dr: '┌',
            dl: '┐',
            ur: '└',
        },
    ),
    (
        "━┃ heavy",
        LineStyle {
            h: '━',
            v: '┃',
            vr: '┣',
            vl: '┫',
            ul: '┛',
            dr: '┏',
            dl: '┓',
            ur: '┗',
        },
    ),
    (
        "═║ double",
        LineStyle {
            h: '═',
            v: '║',
            vr: '╠',
            vl: '╣',
            ul: '╝',
            dr: '╔',
            dl: '╗',
            ur: '╚',
        },
    ),
    (
        "─│ rounded",
        LineStyle {
            h: '─',
            v: '│',
            vr: '├',
            vl: '┤',
            ul: '╯',
            dr: '╭',
            dl: '╮',
            ur: '╰',
        },
    ),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Equal,
    Added,
    Removed,
    Changed,
}

type Segs = Vec<(String, Kind)>;
type Rows = (usize, usize);

enum DiffRow {
    Ctx(String),
    Chg(String, String),
    Del(String),
    Ins(String),
}

fn diff_rows(old: &str, new: &str) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut dels: Vec<String> = Vec::new();
    let mut ins: Vec<String> = Vec::new();
    for change in TextDiff::from_lines(old, new).iter_all_changes() {
        let value = change.value().trim_end_matches(['\n', '\r']).to_string();
        match change.tag() {
            ChangeTag::Equal => {
                flush_rows(&mut dels, &mut ins, &mut rows);
                rows.push(DiffRow::Ctx(value));
            }
            ChangeTag::Delete => dels.push(value),
            ChangeTag::Insert => ins.push(value),
        }
    }
    flush_rows(&mut dels, &mut ins, &mut rows);
    rows
}

fn similarity(a: &str, b: &str) -> f64 {
    let equal: usize = TextDiff::from_chars(a, b)
        .iter_all_changes()
        .filter(|c| c.tag() == ChangeTag::Equal)
        .map(|c| c.value().chars().count())
        .sum();
    let total = a.chars().count().max(b.chars().count()).max(1);
    equal as f64 / total as f64
}

fn flush_rows(dels: &mut Vec<String>, ins: &mut Vec<String>, rows: &mut Vec<DiffRow>) {
    // Pair removed and added lines by similarity (not position), so that a hunk
    // with unequal counts still matches the lines that actually correspond.
    let mut matched: Vec<Option<usize>> = vec![None; dels.len()];
    let mut used = vec![false; ins.len()];
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (i, d) in dels.iter().enumerate() {
        for (j, n) in ins.iter().enumerate() {
            candidates.push((similarity(d, n), i, j));
        }
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, i, j) in candidates {
        if matched[i].is_none() && !used[j] {
            matched[i] = Some(j);
            used[j] = true;
        }
    }
    for (i, line) in dels.iter().enumerate() {
        match matched[i] {
            Some(j) => rows.push(DiffRow::Chg(line.clone(), ins[j].clone())),
            None => rows.push(DiffRow::Del(line.clone())),
        }
    }
    for (j, line) in ins.iter().enumerate() {
        if !used[j] {
            rows.push(DiffRow::Ins(line.clone()));
        }
    }
    dels.clear();
    ins.clear();
}

fn intra_line(old: &str, new: &str) -> (Segs, Segs) {
    let mut o: Segs = Vec::new();
    let mut n: Segs = Vec::new();
    let mut del = String::new();
    let mut ins = String::new();

    for change in TextDiff::from_words(old, new).iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                flush_word(&del, &ins, &mut o, &mut n);
                del.clear();
                ins.clear();
                o.push((change.value().to_string(), Kind::Equal));
                n.push((change.value().to_string(), Kind::Equal));
            }
            ChangeTag::Delete => del.push_str(change.value()),
            ChangeTag::Insert => ins.push_str(change.value()),
        }
    }
    flush_word(&del, &ins, &mut o, &mut n);
    collapse_if_rewritten(&mut o, &mut n);
    (o, n)
}

fn flush_word(del: &str, ins: &str, o: &mut Segs, n: &mut Segs) {
    if del.is_empty() && ins.is_empty() {
        return;
    }
    if ins.is_empty() {
        o.push((del.to_string(), Kind::Removed));
    } else if del.is_empty() {
        n.push((ins.to_string(), Kind::Added));
    } else {
        let mut cdel = String::new();
        let mut cins = String::new();
        for ch in TextDiff::from_chars(del, ins).iter_all_changes() {
            match ch.tag() {
                ChangeTag::Equal => {
                    flush_pair(&cdel, &cins, o, n);
                    cdel.clear();
                    cins.clear();
                    o.push((ch.value().to_string(), Kind::Equal));
                    n.push((ch.value().to_string(), Kind::Equal));
                }
                ChangeTag::Delete => cdel.push_str(ch.value()),
                ChangeTag::Insert => cins.push_str(ch.value()),
            }
        }
        flush_pair(&cdel, &cins, o, n);
    }
}

fn flush_pair(cdel: &str, cins: &str, o: &mut Segs, n: &mut Segs) {
    match (cdel.is_empty(), cins.is_empty()) {
        (false, false) => {
            o.push((cdel.to_string(), Kind::Changed));
            n.push((cins.to_string(), Kind::Changed));
        }
        (false, true) => o.push((cdel.to_string(), Kind::Removed)),
        (true, false) => n.push((cins.to_string(), Kind::Added)),
        (true, true) => {}
    }
}

fn collapse_if_rewritten(o: &mut Segs, n: &mut Segs) {
    let frac = |segs: &Segs| -> f64 {
        let total: usize = segs.iter().map(|(t, _)| t.chars().count()).sum();
        let changed: usize = segs
            .iter()
            .filter(|(_, k)| *k != Kind::Equal)
            .map(|(t, _)| t.chars().count())
            .sum();
        if total == 0 {
            0.0
        } else {
            changed as f64 / total as f64
        }
    };
    if frac(o) > 0.6 && frac(n) > 0.6 {
        let ot: String = o.iter().map(|(t, _)| t.as_str()).collect();
        let nt: String = n.iter().map(|(t, _)| t.as_str()).collect();
        *o = vec![(ot, Kind::Changed)];
        *n = vec![(nt, Kind::Changed)];
    }
}

fn span_style(kind: Kind, old_side: bool) -> (Option<Color>, Option<Color>, Modifier) {
    let (fg, bg) = match kind {
        Kind::Removed => (REMOVED_FG, REMOVED_BG),
        Kind::Added => (ADDED_FG, ADDED_BG),
        Kind::Changed => (CHANGED_FG, CHANGED_BG),
        Kind::Equal => return (None, None, Modifier::empty()),
    };
    let modifier = match kind {
        Kind::Removed => Modifier::CROSSED_OUT,
        Kind::Added => Modifier::UNDERLINED,
        _ if old_side => Modifier::CROSSED_OUT,
        _ => Modifier::UNDERLINED,
    };
    (Some(bg), Some(fg), modifier | Modifier::BOLD)
}

fn styled(text: String, kind: Kind, old_side: bool, base: Color) -> Span<'static> {
    let (bg, fg, modifier) = span_style(kind, old_side);
    let mut span = Span::styled(text, Style::default()).add_modifier(modifier);
    span = span.bg(bg.unwrap_or(base));
    if let Some(fg) = fg {
        span = span.fg(fg);
    }
    span
}

fn ctx(t: &str, width: usize) -> Line<'static> {
    let used = t.chars().count();
    Line::from(vec![
        Span::raw(t.to_string()),
        Span::raw(" ".repeat(width.saturating_sub(used))),
    ])
}

fn modified_line(old: &str, new: &str, old_side: bool, width: usize) -> Line<'static> {
    let (o, n) = intra_line(old, new);
    let segs = if old_side { &o } else { &n };

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for (text, kind) in segs {
        used += text.chars().count();
        spans.push(styled(text.clone(), *kind, old_side, MODIFIED_LINE));
    }
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))).bg(MODIFIED_LINE));
    Line::from(spans)
}

fn blank(width: usize) -> Line<'static> {
    Line::from(Span::raw(" ".repeat(width.max(1))))
}

fn side_line(text: &str, added: bool, old_side: bool, width: usize) -> Line<'static> {
    let present = if added { !old_side } else { old_side };
    if !present {
        return blank(width);
    }
    let (fg, modifier, base) = if added {
        (ADDED_FG, Modifier::UNDERLINED, ADDED_LINE)
    } else {
        (REMOVED_FG, Modifier::CROSSED_OUT, REMOVED_LINE)
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    used += text.chars().count();
    spans.push(
        Span::styled(text.to_string(), Style::default().fg(fg))
            .add_modifier(modifier)
            .bg(base),
    );
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))).bg(base));
    Line::from(spans)
}

fn build_side(rows: &[DiffRow], old_side: bool, width: usize) -> Vec<Line<'static>> {
    // TEMPORARY: show file line numbers (old panel on the left, new panel on
    // the right) to make talking about specific lines easier.
    let total = rows
        .iter()
        .filter(|r| match r {
            DiffRow::Ctx(_) | DiffRow::Chg(_, _) => true,
            DiffRow::Del(_) => old_side,
            DiffRow::Ins(_) => !old_side,
        })
        .count();
    let num_w = total.max(1).to_string().len();
    let code_w = width.saturating_sub(num_w + 1);
    let num_style = Style::new().fg(Color::Rgb(0x5c, 0x66, 0x70));

    let mut out = Vec::new();
    let mut lineno = 0usize;
    for row in rows {
        let line = match row {
            DiffRow::Ctx(t) => {
                lineno += 1;
                Some(ctx(t, code_w))
            }
            DiffRow::Chg(o, n) => {
                lineno += 1;
                Some(modified_line(o, n, old_side, code_w))
            }
            DiffRow::Del(t) => {
                if old_side {
                    lineno += 1;
                    Some(side_line(t, false, old_side, code_w))
                } else {
                    None
                }
            }
            DiffRow::Ins(t) => {
                if !old_side {
                    lineno += 1;
                    Some(side_line(t, true, old_side, code_w))
                } else {
                    None
                }
            }
        };
        let Some(line) = line else { continue };
        let label = format!("{:>num_w$}", lineno);
        let mut spans = Vec::new();
        if old_side {
            spans.push(Span::styled(format!("{label} "), num_style));
            spans.extend(line.spans);
        } else {
            spans.extend(line.spans);
            spans.push(Span::styled(format!(" {label}"), num_style));
        }
        out.push(Line::from(spans));
    }
    out
}

// ---- gutter ----

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

/// Draw a stepped line in the gutter from (c0row, col 0) to (c3row, the last
/// column), joining a before-side rail to its after-side counterpart. `spine`
/// is the column the vertical part runs along, so different connectors can use
/// different lanes and never overlap.
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
/// row range so the line leaves the first line, the last line, or the middle
/// line as appropriate.
fn rail_attach_row(arrow_row: i32, lo: i32, hi: i32, height: i32) -> usize {
    let lo = lo.max(0);
    let hi = hi.min(height - 1).max(lo);
    arrow_row.clamp(lo, hi) as usize
}

fn leading_ws(text: &str) -> usize {
    text.chars().take_while(|c| *c == ' ').count()
}

/// New-side line texts in display order.
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

/// Old-side line texts in display order.
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

/// A cell painted into a panel after the normal render: a rail, a gap arrow,
/// or the horizontal bridge that links a rail/arrow to the gutter.
#[derive(Clone, Copy)]
struct Ov {
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

#[allow(clippy::too_many_arguments)]
fn build_gutter(
    rows: &[DiffRow],
    end_top: char,
    end_bot: char,
    rail_left: char,
    rail_right: char,
    style: LineStyle,
    delta: i32,
    old_w: usize,
    new_w: usize,
) -> (Vec<Line<'static>>, Vec<Ov>) {
    let width = GAP as usize;
    let new_texts = new_side_texts(rows);
    let old_texts = old_side_texts(rows);
    // Old panel text is prefixed by its right-aligned line number + a space.
    let old_off = old_texts.len().max(1).to_string().len() + 1;
    let mut ovs: Vec<Ov> = Vec::new();
    let mut cells: Vec<Vec<Cell>> = Vec::new();
    // Each block: its rows in the old view, and its rows in the new view.
    // Removed lines have an empty new-view range; added lines an empty old one.
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
        // Stepping connector inside the gutter, then bridges out to both panels.
        connector_edge(&mut cells, c0row, c3row, spine, color, style);
        cells[c0row][0] = glyph(style.h, color);
        cells[c3row][width - 1] = glyph(style.h, color);

        // ---- old (left) panel: rail hugs the end of the old code ----
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

        // ---- new (right) panel: rail hugs the start of the new code ----
        if !removed {
            let start = (rs..re)
                .filter_map(|r| new_texts.get(r))
                .map(|t| leading_ws(t))
                .min()
                .unwrap_or(0);
            let rc = start.saturating_sub(2).min(new_w.saturating_sub(1));
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
    (cells.iter().map(|row| cells_to_line(row)).collect(), ovs)
}

#[derive(Clone)]
struct Opts {
    end_top: char,
    end_bot: char,
    end_name: &'static str,
    rail_left: char,
    rail_left_name: String,
    rail_right: char,
    rail_right_name: String,
    line: LineStyle,
    line_name: &'static str,
    delta: i32,
}

fn rail_label(rail: usize, custom: Option<char>) -> String {
    match custom {
        Some(c) => format!("custom U+{:04X}", c as u32),
        None => RAIL_NAMES[rail].to_string(),
    }
}

fn ui(f: &mut Frame, rows: &[DiffRow], scroll: u16, opts: Opts) {
    let [main, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
    let [old, gap, new] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(GAP),
        Constraint::Min(0),
    ])
    .areas(main);

    // a header line per column, then the body (no frames)
    let [old_head, old_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(old);
    let [_, gap_body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(gap);
    let [new_head, new_body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(new);

    let old_w = old_body.width as usize;
    let new_w = new_body.width as usize;
    let right_scroll = (scroll as i32 + opts.delta).max(0) as u16;

    f.render_widget(
        Paragraph::new(format!(" old   {}", opts.rail_left_name)).style(help_style()),
        old_head,
    );
    f.render_widget(
        Paragraph::new(format!(
            " new   {}   {}   {}   d={}",
            opts.rail_right_name, opts.end_name, opts.line_name, opts.delta
        ))
        .style(help_style()),
        new_head,
    );

    f.render_widget(
        Paragraph::new(build_side(rows, true, old_w)).scroll((scroll, 0)),
        old_body,
    );
    f.render_widget(
        Paragraph::new(build_side(rows, false, new_w)).scroll((right_scroll, 0)),
        new_body,
    );

    let (gutter_lines, overlays) = build_gutter(
        rows,
        opts.end_top,
        opts.end_bot,
        opts.rail_left,
        opts.rail_right,
        opts.line,
        opts.delta,
        old_w,
        new_w,
    );
    f.render_widget(Paragraph::new(gutter_lines).scroll((scroll, 0)), gap_body);

    draw_overlays(f, &overlays, old_body, new_body, scroll);

    f.render_widget(Paragraph::new(HELP_MAIN).style(help_style()), help);
}

fn paint(f: &mut Frame, x: u16, y: u16, ch: char, color: Color) {
    let cell = &mut f.buffer_mut()[(x, y)];
    cell.set_char(ch);
    cell.set_fg(color);
}

/// Paint the rails, gap arrows and the horizontal bridges that join them to the
/// gutter, directly into the panels.
fn draw_overlays(f: &mut Frame, ovs: &[Ov], old_body: Rect, new_body: Rect, scroll: u16) {
    for ov in ovs {
        let body = if ov.left { old_body } else { new_body };
        if body.width == 0 || body.height == 0 {
            continue;
        }
        let top = body.y as i32;
        let bottom = (body.y + body.height) as i32;
        let y = top + ov.row - scroll as i32;
        if y < top || y >= bottom || ov.col >= body.width as usize {
            continue;
        }
        paint(f, body.x + ov.col as u16, y as u16, ov.ch, ov.color);
    }
}

fn main() -> io::Result<()> {
    let mut rows = load_rows()?;
    let mut terminal = ratatui::init();
    let mut scroll = 0u16;
    let mut delta = 0i32;
    loop {
        terminal.draw(|f| {
            ui(
                f,
                &rows,
                scroll,
                Opts {
                    end_top: END_PAIRS[0].1,
                    end_bot: END_PAIRS[0].2,
                    end_name: END_PAIRS[0].0,
                    rail_left: RAILS[9],
                    rail_left_name: rail_label(9, None),
                    rail_right: RAILS[9],
                    rail_right_name: rail_label(9, None),
                    line: LINES[3].1,
                    line_name: LINES[3].0,
                    delta,
                },
            );
        })?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('j') | KeyCode::Down => scroll = scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => scroll = scroll.saturating_add(20),
                KeyCode::PageUp => scroll = scroll.saturating_sub(20),
                KeyCode::Home | KeyCode::Char('G') | KeyCode::Char('c') => scroll = 0,
                KeyCode::Char('.') => delta += 1,
                KeyCode::Char(',') => delta -= 1,
                KeyCode::Char('R') => {
                    if let Ok(reloaded) = load_rows() {
                        rows = reloaded;
                    }
                }
                _ => {}
            }
        }
    }
    ratatui::restore();
    Ok(())
}

#[cfg(test)]
mod tests {
    // Sample files live in `samples/before.txt` and `samples/after.txt` and
    // are meant to be edited freely, so tests here should not depend on
    // their contents.
}
