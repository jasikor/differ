//! Colours, glyphs and line styles. Defaults frozen from `mockup/NOTES.md`.

use ratatui::style::{Color, Style};

/// Width of the inter-pane gutter.
pub const GAP: u16 = 10;

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

/// End-marker pairs for removed blocks (top glyph, bottom glyph).
pub const END_PAIRS: [(&str, char, char); 9] = [
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

/// Mirrored arrow pair for the added-lines gap, which sits on the before side.
pub const ADD_TOP: char = '\u{1FB48}';
pub const ADD_BOT: char = '\u{1FB65}';

/// Rail glyphs. U+258x partial blocks are the classic rails.
pub const RAILS: [char; 10] = ['▏', '▎', '▍', '▌', '▐', '▕', '█', '┃', '║', '│'];

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

pub const LINES: [(&str, LineStyle); 4] = [
    (
        "light",
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
        "heavy",
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
        "double",
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
        "rounded",
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

/// The frozen visual defaults.
#[derive(Clone, Copy)]
pub struct ViewOpts {
    pub end_top: char,
    pub end_bot: char,
    pub rail_left: char,
    pub rail_right: char,
    pub line: LineStyle,
    pub delta: i32,
}

impl Default for ViewOpts {
    fn default() -> Self {
        ViewOpts {
            end_top: END_PAIRS[0].1,
            end_bot: END_PAIRS[0].2,
            rail_left: RAILS[9],
            rail_right: RAILS[9],
            line: LINES[3].1,
            delta: 0,
        }
    }
}

pub fn dim_style() -> Style {
    Style::new().fg(Color::Rgb(0x8c, 0x96, 0xa0))
}

pub fn num_style() -> Style {
    Style::new().fg(Color::Rgb(0x5c, 0x66, 0x70))
}
