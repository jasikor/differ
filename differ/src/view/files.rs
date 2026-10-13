//! The left file list.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::git::FileEntry;

pub fn draw(f: &mut Frame, files: &[FileEntry], selected: usize, scroll: u16, area: Rect) {
    let lines: Vec<Line> = files
        .iter()
        .enumerate()
        .map(|(i, fe)| {
            let x = fe.staged.unwrap_or('.');
            let y = fe.unstaged.unwrap_or('.');
            let text = format!("{x}{y} {}", fe.path);
            let style = if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}
