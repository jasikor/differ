//! The two code panes (old on the left, new on the right).

use ratatui::{
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
};

use crate::model::{intra_line, DiffRow, Kind, Segs};
use crate::view::theme::{
    ADDED_BG, ADDED_FG, ADDED_LINE, CHANGED_BG, CHANGED_FG, MODIFIED_LINE, NEW_MARGIN, OLD_MARGIN,
    REMOVED_BG, REMOVED_FG, REMOVED_LINE,
};

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
    let segs: &Segs = if old_side { &o } else { &n };

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

/// Truncate a run of spans to at most `max` characters, so long code never
/// overruns the rail.
fn clip(mut spans: Vec<Span<'static>>, max: usize) -> Vec<Span<'static>> {
    let mut used = 0usize;
    for (i, sp) in spans.iter().enumerate() {
        let len = sp.content.chars().count();
        if used + len > max {
            let keep = max - used;
            let text: String = sp.content.chars().take(keep).collect();
            let style = sp.style;
            spans.truncate(i);
            if keep > 0 {
                spans.push(Span::styled(text, style));
            }
            return spans;
        }
        used += len;
    }
    spans
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
    // Keep the leading indentation un-emphasised: only the text itself is
    // struck through (removed) or underlined (added).
    let lead_len = text.len() - text.trim_start().len();
    let (lead, rest) = text.split_at(lead_len);
    if !lead.is_empty() {
        used += lead.chars().count();
        spans.push(Span::styled(lead.to_string(), Style::default().bg(base)));
    }
    if !rest.is_empty() {
        used += rest.chars().count();
        spans.push(
            Span::styled(rest.to_string(), Style::default().fg(fg))
                .add_modifier(modifier)
                .bg(base),
        );
    }
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))).bg(base));
    Line::from(spans)
}

/// Width of the right-aligned line-number label for `total` lines.
pub fn num_width(total: usize) -> usize {
    total.max(1).to_string().len()
}

/// Build one pane's lines (line numbers included) from the diff rows.
pub fn build_side(rows: &[DiffRow], old_side: bool, width: usize) -> Vec<Line<'static>> {
    let total = rows
        .iter()
        .filter(|r| match r {
            DiffRow::Ctx(_) | DiffRow::Chg(_, _) => true,
            DiffRow::Del(_) => old_side,
            DiffRow::Ins(_) => !old_side,
        })
        .count();
    let num_w = num_width(total);
    let code_w = width.saturating_sub(num_w + 1 + if old_side { OLD_MARGIN } else { NEW_MARGIN });
    let num_style = crate::view::theme::num_style();

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
        let line = clip(line.spans, code_w);
        let label = format!("{lineno:>num_w$}");
        let mut spans = Vec::new();
        if old_side {
            spans.push(Span::styled(format!("{label} "), num_style));
            spans.extend(line);
        } else {
            spans.push(Span::raw(" ".repeat(NEW_MARGIN)));
            spans.extend(line);
            spans.push(Span::styled(format!(" {label}"), num_style));
        }
        out.push(Line::from(spans));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(line: &Line) -> usize {
        line.spans.iter().map(|s| s.content.chars().count()).sum()
    }

    fn demo_rows() -> Vec<DiffRow> {
        vec![
            DiffRow::Ctx("abc".into()),
            DiffRow::Del("    gone".into()),
            DiffRow::Ins("    added".into()),
            DiffRow::Chg("old".into(), "new".into()),
        ]
    }

    #[test]
    fn each_side_skips_the_other_sides_exclusive_rows() {
        let rows = demo_rows();
        // old: Ctx, Del, Chg (Ins dropped); new: Ctx, Ins, Chg (Del dropped).
        assert_eq!(build_side(&rows, true, 20).len(), 3);
        assert_eq!(build_side(&rows, false, 20).len(), 3);
    }

    #[test]
    fn old_side_reserves_its_margin_the_new_side_indents() {
        let rows = demo_rows();
        let old = build_side(&rows, true, 20);
        let new = build_side(&rows, false, 20);
        // Both panes render exactly `width` columns once their margin is added.
        assert_eq!(width(&old[0]), 20 - OLD_MARGIN);
        assert_eq!(width(&new[0]), 20);
        assert!(new[0].spans[0].content.starts_with(' '));
    }

    #[test]
    fn long_lines_are_clipped_to_the_code_width() {
        let rows = vec![DiffRow::Ctx("x".repeat(80))];
        let old = build_side(&rows, true, 20);
        assert_eq!(width(&old[0]), 20 - OLD_MARGIN);
    }

    #[test]
    fn num_width_tracks_the_digit_count() {
        assert_eq!(num_width(0), 1);
        assert_eq!(num_width(9), 1);
        assert_eq!(num_width(10), 2);
        assert_eq!(num_width(999), 3);
    }
}
