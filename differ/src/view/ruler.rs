//! Overview ruler: a one-column map of a pane's lines.
//!
//! Each cell covers a slice of the pane's lines. Hunk lines are coloured (by
//! kind); the visible slice of the file is drawn brighter than the rest, so you
//! can tell at a glance where you are and where the changes are.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::model::Kind;
use crate::view::theme::{hunk_color, RULER_BRIGHT, RULER_DIM};

pub fn draw(f: &mut Frame, area: Rect, kinds: &[Option<Kind>], scroll: u16) {
    let h = area.height as usize;
    if h == 0 {
        return;
    }
    let total = kinds.len();
    let view_top = scroll as usize;
    let view_bot = view_top + h;

    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for i in 0..h {
        // Map this ruler row to a line range [s, e) in the pane.
        let (s, e) = if total <= h {
            (i, (i + 1).min(total))
        } else {
            let s = i * total / h;
            let e = ((i + 1) * total).div_ceil(h).max(s + 1).min(total);
            (s, e)
        };

        let (ch, color) = if s >= total {
            // Past the end of the file.
            ('│', RULER_DIM)
        } else {
            let kind = kinds[s..e].iter().flatten().next().copied();
            let in_view = s < view_bot && e > view_top;
            match kind {
                // Hunk colours are the same whether or not they're on screen.
                Some(k) => ('█', hunk_color(k)),
                None => ('│', if in_view { RULER_BRIGHT } else { RULER_DIM }),
            }
        };
        lines.push(Line::from(Span::styled(
            ch.to_string(),
            Style::default().fg(color),
        )));
    }
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn marks_hunk_cells() {
        // Pane 10 rows tall, 10 lines -> 1:1 mapping; hunk on lines 5-6.
        let mut kinds = vec![None; 10];
        kinds[5] = Some(Kind::Added);
        kinds[6] = Some(Kind::Added);
        let mut t = Terminal::new(TestBackend::new(1, 10)).unwrap();
        t.draw(|f| draw(f, f.area(), &kinds, 0)).unwrap();
        let buf = t.backend().buffer().clone();
        assert_eq!(buf.cell((0, 5)).unwrap().symbol(), "█");
        assert_eq!(buf.cell((0, 6)).unwrap().symbol(), "█");
        assert_eq!(buf.cell((0, 0)).unwrap().symbol(), "│");
    }
}
