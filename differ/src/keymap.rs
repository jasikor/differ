//! Key bindings: a key *sequence* → [`Command`], so vim-style multi-key
//! mappings (`gg`, `]c`, …) can be added as data without touching the event
//! loop. The default table aims to read like Neovim.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::command::Command;

/// A key press: the code plus the one modifier we care about (Ctrl). The
/// default table uses Ctrl for panel switching, leaving plain keys for motions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
}

impl Key {
    pub const fn plain(code: KeyCode) -> Self {
        Key { code, ctrl: false }
    }

    pub const fn ctrl(code: KeyCode) -> Self {
        Key { code, ctrl: true }
    }

    pub fn from_event(event: KeyEvent) -> Self {
        Key {
            code: event.code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        }
    }
}

/// What feeding a key to the keymap produced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// A binding completed.
    Run(Command),
    /// The keys so far are a prefix of one or more bindings.
    Pending,
    /// No binding matches.
    None,
}

pub struct Keymap {
    bindings: Vec<(Vec<Key>, Command)>,
    pending: Vec<Key>,
}

impl Keymap {
    pub fn new(bindings: Vec<(Vec<Key>, Command)>) -> Self {
        Keymap {
            bindings,
            pending: Vec::new(),
        }
    }

    /// The default, Neovim-flavoured bindings.
    pub fn default_bindings() -> Self {
        use KeyCode::*;
        let one = |c: KeyCode, cmd: Command| (vec![Key::plain(c)], cmd);
        let ctrl = |c: KeyCode, cmd: Command| (vec![Key::ctrl(c)], cmd);
        let two =
            |a: char, b: char, cmd: Command| (vec![Key::plain(Char(a)), Key::plain(Char(b))], cmd);
        Keymap::new(vec![
            one(Char('q'), Command::Quit),
            one(Char('h'), Command::Left),
            one(Left, Command::Left),
            one(Char('j'), Command::Down),
            one(Down, Command::Down),
            one(Char('k'), Command::Up),
            one(Up, Command::Up),
            one(Char('l'), Command::Right),
            one(Right, Command::Right),
            // Cycle panels, vim-window style.
            ctrl(Char('h'), Command::FocusPrev),
            ctrl(Char('l'), Command::FocusNext),
            // Scroll the focused pane's view a line (vim's Ctrl-e / Ctrl-y).
            ctrl(Char('e'), Command::ScrollDown),
            ctrl(Char('y'), Command::ScrollUp),
            // Ctrl-z zooms the focused diff pane to the full width.
            ctrl(Char('z'), Command::ToggleZoom),
            // Space is the leader; Space e toggles the file (git) panel.
            two(' ', 'e', Command::ToggleFiles),
            // s jumps to a visible character (flash-style).
            one(Char('s'), Command::StartJump),
            // Hunk navigation, vim-diff style.
            two(']', 'c', Command::NextHunk),
            two('[', 'c', Command::PrevHunk),
            // Word motions (vim's w/b/e) and line ends (0/^/$).
            one(Char('w'), Command::WordForward),
            one(Char('b'), Command::WordBack),
            one(Char('e'), Command::WordEnd),
            // WORD motions (vim's W/B/E): whitespace-delimited.
            one(Char('W'), Command::WordForwardBig),
            one(Char('B'), Command::WordBackBig),
            one(Char('E'), Command::WordEndBig),
            one(Char('0'), Command::LineStart),
            one(Char('^'), Command::LineFirstNonBlank),
            one(Char('$'), Command::LineEnd),
            // Put the cursor line at the middle / top / bottom (vim's zz/zt/zb).
            two('z', 'z', Command::CursorCenter),
            two('z', 't', Command::CursorTop),
            two('z', 'b', Command::CursorBottom),
            one(PageDown, Command::PageDown),
            one(PageUp, Command::PageUp),
            // Jump to the top / bottom of the focused region (vim's gg / G).
            two('g', 'g', Command::Top),
            one(Char('G'), Command::Bottom),
            one(Char('R'), Command::Reload),
        ])
    }

    /// Feed one key. Stateful: it accumulates keys until a binding resolves.
    /// Escape always cancels a sequence in progress (vim-style); it never quits.
    pub fn feed(&mut self, key: Key) -> Resolution {
        if key.code == KeyCode::Esc {
            self.pending.clear();
            return Resolution::None;
        }
        self.pending.push(key);
        let is_prefix = self
            .bindings
            .iter()
            .any(|(seq, _)| seq.len() > self.pending.len() && seq.starts_with(&self.pending));
        if is_prefix {
            return Resolution::Pending;
        }
        if let Some((_, cmd)) = self.bindings.iter().find(|(seq, _)| *seq == self.pending) {
            let cmd = *cmd;
            self.pending.clear();
            return Resolution::Run(cmd);
        }
        self.pending.clear();
        Resolution::None
    }

    /// The keys buffered so far (for a pending sequence). Used by the UI once
    /// pending sequences exist (vim's `showcmd`).
    #[allow(dead_code)]
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(c: char) -> Key {
        Key::plain(KeyCode::Char(c))
    }

    #[test]
    fn single_key_matches_immediately() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('q')), Resolution::Run(Command::Quit));
        assert!(km.pending().is_empty());
    }

    #[test]
    fn ctrl_h_and_ctrl_l_cycle_panels() {
        let mut km = Keymap::default_bindings();
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('h'))),
            Resolution::Run(Command::FocusPrev)
        );
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('l'))),
            Resolution::Run(Command::FocusNext)
        );
        // The plain letters are the motions, not panel focus.
        assert_eq!(km.feed(plain('h')), Resolution::Run(Command::Left));
        assert_eq!(km.feed(plain('l')), Resolution::Run(Command::Right));
    }

    #[test]
    fn z_prefix_places_the_cursor_line() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('z')), Resolution::Pending);
        assert_eq!(km.feed(plain('z')), Resolution::Run(Command::CursorCenter));
        assert_eq!(km.feed(plain('z')), Resolution::Pending);
        assert_eq!(km.feed(plain('t')), Resolution::Run(Command::CursorTop));
        assert_eq!(km.feed(plain('z')), Resolution::Pending);
        assert_eq!(km.feed(plain('b')), Resolution::Run(Command::CursorBottom));
    }

    #[test]
    fn ctrl_e_and_ctrl_y_scroll() {
        let mut km = Keymap::default_bindings();
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('e'))),
            Resolution::Run(Command::ScrollDown)
        );
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('y'))),
            Resolution::Run(Command::ScrollUp)
        );
        // Plain e is its own motion (word end), not scrolling; y is free.
        assert_eq!(km.feed(plain('e')), Resolution::Run(Command::WordEnd));
        assert_eq!(km.feed(plain('y')), Resolution::None);
    }

    #[test]
    fn ctrl_z_toggles_zoom() {
        let mut km = Keymap::default_bindings();
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('z'))),
            Resolution::Run(Command::ToggleZoom)
        );
    }

    #[test]
    fn leader_space_e_toggles_the_file_panel() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        assert_eq!(km.feed(plain('e')), Resolution::Run(Command::ToggleFiles));
        // A leader followed by a non-binding key clears without running.
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        assert_eq!(km.feed(plain('j')), Resolution::None);
        // Plain e is the word-end motion.
        assert_eq!(km.feed(plain('e')), Resolution::Run(Command::WordEnd));
    }

    #[test]
    fn gg_and_shift_g_jump_to_top_and_bottom() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(km.feed(plain('g')), Resolution::Run(Command::Top));
        assert_eq!(km.feed(plain('G')), Resolution::Run(Command::Bottom));
        // A lone g followed by something else clears without running.
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(km.feed(plain('j')), Resolution::None);
    }

    #[test]
    fn word_line_and_hunk_motions_are_bound() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('w')), Resolution::Run(Command::WordForward));
        assert_eq!(km.feed(plain('b')), Resolution::Run(Command::WordBack));
        assert_eq!(km.feed(plain('e')), Resolution::Run(Command::WordEnd));
        assert_eq!(
            km.feed(plain('W')),
            Resolution::Run(Command::WordForwardBig)
        );
        assert_eq!(km.feed(plain('B')), Resolution::Run(Command::WordBackBig));
        assert_eq!(km.feed(plain('E')), Resolution::Run(Command::WordEndBig));
        assert_eq!(km.feed(plain('0')), Resolution::Run(Command::LineStart));
        assert_eq!(
            km.feed(plain('^')),
            Resolution::Run(Command::LineFirstNonBlank)
        );
        assert_eq!(km.feed(plain('$')), Resolution::Run(Command::LineEnd));
        assert_eq!(km.feed(plain(']')), Resolution::Pending);
        assert_eq!(km.feed(plain('c')), Resolution::Run(Command::NextHunk));
        assert_eq!(km.feed(plain('[')), Resolution::Pending);
        assert_eq!(km.feed(plain('c')), Resolution::Run(Command::PrevHunk));
        // The old single-key hunk motions are gone.
        assert_eq!(km.feed(plain('n')), Resolution::None);
    }

    #[test]
    fn s_starts_a_jump() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('s')), Resolution::Run(Command::StartJump));
    }

    #[test]
    fn unknown_key_is_none() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('x')), Resolution::None);
    }

    #[test]
    fn escape_cancels_a_pending_sequence_without_quitting() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(km.feed(Key::plain(KeyCode::Esc)), Resolution::None);
        assert!(km.pending().is_empty());
        // Escape on its own is a no-op, not a quit.
        assert_eq!(km.feed(Key::plain(KeyCode::Esc)), Resolution::None);
    }

    #[test]
    fn sequences_pend_then_run() {
        let mut km = Keymap::new(vec![
            (vec![plain('g'), plain('g')], Command::Top),
            (vec![plain('g'), plain('h')], Command::Up),
        ]);
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(km.pending(), &[plain('g')]);
        assert_eq!(km.feed(plain('g')), Resolution::Run(Command::Top));
        assert!(km.pending().is_empty());
    }
}
