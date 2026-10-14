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
            one(Esc, Command::Quit),
            one(Char('j'), Command::Down),
            one(Down, Command::Down),
            one(Char('k'), Command::Up),
            one(Up, Command::Up),
            // Cycle panels, vim-window style.
            ctrl(Char('h'), Command::FocusPrev),
            ctrl(Char('l'), Command::FocusNext),
            // Scroll the focused pane's view a line (vim's Ctrl-e / Ctrl-y).
            ctrl(Char('e'), Command::ScrollDown),
            ctrl(Char('y'), Command::ScrollUp),
            // Ctrl-z zooms the focused diff pane to the full width.
            ctrl(Char('z'), Command::ToggleZoom),
            one(Char('n'), Command::NextHunk),
            one(Char('p'), Command::PrevHunk),
            // Put the cursor line at the middle / top / bottom (vim's zz/zt/zb).
            two('z', 'z', Command::CursorCenter),
            two('z', 't', Command::CursorTop),
            two('z', 'b', Command::CursorBottom),
            one(PageDown, Command::PageDown),
            one(PageUp, Command::PageUp),
            one(Home, Command::Top),
            one(Char('g'), Command::Top),
            one(Char('R'), Command::Reload),
        ])
    }

    /// Feed one key. Stateful: it accumulates keys until a binding resolves.
    pub fn feed(&mut self, key: Key) -> Resolution {
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
        // The plain letters must stay available for motions, not panel focus.
        assert_eq!(km.feed(plain('l')), Resolution::None);
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
        // Plain e/y stay free.
        assert_eq!(km.feed(plain('e')), Resolution::None);
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
    fn unknown_key_is_none() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('x')), Resolution::None);
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
