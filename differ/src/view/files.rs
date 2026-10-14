//! The left file tree.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tree::Row;
use crate::view::theme;

pub fn draw(f: &mut Frame, rows: &[Row], selected: Option<usize>, scroll: u16, area: Rect) {
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let indent = match row {
                Row::Dir { depth, .. } | Row::File { depth, .. } => "  ".repeat(*depth),
            };
            match row {
                Row::Dir { name, .. } => {
                    Line::from(Span::styled(format!("{indent}{name}/"), theme::dim_style()))
                }
                Row::File {
                    name,
                    staged,
                    unstaged,
                    ..
                } => {
                    let x = staged.unwrap_or('.');
                    let y = unstaged.unwrap_or('.');
                    let style = if Some(i) == selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(format!("{indent}{x}{y} {name}"), style))
                }
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}
