//! Colours, glyphs and line styles. Defaults frozen from the original design.

use ratatui::style::{Color, Modifier, Style};

/// Width of the inter-pane gutter (the maximum; the actual width adapts to the
/// number of connectors a diff needs).
pub const GAP: u16 = 10;

/// Columns each pane keeps clear of the code so the rail is never flush against
/// the text: the old pane leaves one space before its (right) rail; the new pane
/// reserves two columns (rail + space) at its left, indenting the code.
pub const OLD_MARGIN: usize = 2;
pub const NEW_MARGIN: usize = 2;

/// Smallest inter-pane gutter (a single connector lane plus the two edges).
pub const MIN_GAP: u16 = 3;

// Backgrounds: faint tints of the line / row kind.
pub const REMOVED_LINE: Color = Color::Rgb(0x2e, 0x22, 0x22);
pub const ADDED_LINE: Color = Color::Rgb(0x1e, 0x2a, 0x24);
pub const MODIFIED_LINE: Color = Color::Rgb(0x1e, 0x1e, 0x1e);

// Span backgrounds / foregrounds: removed=red, added=green, changed=blue.
pub const REMOVED_BG: Color = Color::Rgb(0x4a, 0x2a, 0x2a);
pub const ADDED_BG: Color = Color::Rgb(0x29, 0x47, 0x34);
pub const CHANGED_BG: Color = Color::Rgb(0x26, 0x40, 0x5c);
pub const REMOVED_FG: Color = Color::Rgb(0xe0, 0x6c, 0x75);
pub const ADDED_FG: Color = Color::Rgb(0x98, 0xc3, 0x79);
pub const CHANGED_FG: Color = Color::Rgb(0x6c, 0xb6, 0xff);

// Gutter rails / arrows / connector lines: a muted version of the span
// foregrounds so they read clearly without overpowering the code.
pub const GUTTER_REMOVED: Color = Color::Rgb(0xa8, 0x51, 0x58);
pub const GUTTER_ADDED: Color = Color::Rgb(0x72, 0x92, 0x5b);
pub const GUTTER_CHANGED: Color = Color::Rgb(0x51, 0x89, 0xbf);

// In-pane rails / connector lines / arrows: a dimmer copy of the `GUTTER_*`
// marks, so the structure reads without pulling attention from the code. The
// overview ruler keeps the brighter `GUTTER_*` colours (via `hunk_color`).
pub const RAIL_REMOVED: Color = Color::Rgb(0x4d, 0x25, 0x28);
pub const RAIL_ADDED: Color = Color::Rgb(0x33, 0x42, 0x29);
pub const RAIL_CHANGED: Color = Color::Rgb(0x25, 0x3e, 0x56);

/// End markers for a removed block (drawn beside its first/last visible line).
pub const END_TOP: char = '\u{1FB3D}';
pub const END_BOT: char = '\u{1FB5A}';

/// Removed-block arrow pair for the flipped orientation (block above the gap):
/// tall triangle on top, short below.
pub const END_TOP_FLIP: char = '\u{1FB3F}';
pub const END_BOT_FLIP: char = '\u{1FB58}';

/// Mirrored arrow pair for the added-lines gap, which sits on the before side.
pub const ADD_TOP: char = '\u{1FB48}';
pub const ADD_BOT: char = '\u{1FB65}';

/// Added-block arrow pair for the flipped orientation (added lines above the
/// gap): mirror of `END_TOP_FLIP`/`END_BOT_FLIP`.
pub const ADD_TOP_FLIP: char = '\u{1FB4A}';
pub const ADD_BOT_FLIP: char = '\u{1FB63}';

/// The rail glyph: a thin vertical bar.
pub const RAIL: char = '│';

/// Connector line style: horizontal, vertical, and the four corners.
#[derive(Clone, Copy)]
pub struct LineStyle {
    pub h: char,
    pub v: char,
    pub vr: char,
    pub vl: char,
    pub ul: char,
    pub dr: char,
    pub dl: char,
    pub ur: char,
}

/// The connector line style: rounded corners, thin strokes.
pub const LINE: LineStyle = LineStyle {
    h: '─',
    v: '│',
    vr: '├',
    vl: '┤',
    ul: '╯',
    dr: '╭',
    dl: '╮',
    ur: '╰',
};

pub fn dim_style() -> Style {
    Style::new().fg(Color::Rgb(0x8c, 0x96, 0xa0))
}

/// Header style for the focused region.
pub fn active_style() -> Style {
    Style::new()
        .fg(Color::Rgb(0xe6, 0xea, 0xef))
        .add_modifier(Modifier::BOLD)
}

pub fn num_style() -> Style {
    Style::new().fg(Color::Rgb(0x5c, 0x66, 0x70))
}

// Overview ruler: a neutral track that brightens inside the viewport, and hunk
// cells coloured by kind.
pub const RULER_BRIGHT: Color = Color::Rgb(0xc8, 0xce, 0xd4);
pub const RULER_DIM: Color = Color::Rgb(0x44, 0x49, 0x4e);

pub fn hunk_color(kind: crate::model::Kind) -> Color {
    match kind {
        crate::model::Kind::Removed => GUTTER_REMOVED,
        crate::model::Kind::Added => GUTTER_ADDED,
        crate::model::Kind::Changed | crate::model::Kind::Equal => GUTTER_CHANGED,
    }
}

// Cursor: a solid block for the cursor cell; the cursor line gets one faint
// tint (same in both panes).
pub const CURSOR_ACTIVE: Color = Color::Rgb(0xff, 0xbf, 0x00);
pub const CURSOR_INACTIVE: Color = Color::Rgb(0x4e, 0x57, 0x63);
pub const CURSOR_LINE: Color = Color::Rgb(0x1b, 0x1f, 0x24);

/// Lighten a colour by `d` per channel (keeps the hue of change tints).
pub fn brighten(c: Color, d: u8) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(d),
            g.saturating_add(d),
            b.saturating_add(d),
        ),
        other => other,
    }
}
