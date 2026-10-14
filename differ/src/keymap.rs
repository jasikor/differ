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
        Keymap::new(vec![
            one(Char('q'), Command::Quit),
            one(Esc, Command::Quit),
            one(Char('j'), Command::Down),
            one(Down, Command::Down),
            one(Char('k'), Command::Up),
            one(Up, Command::Up),
            // Panels: digits, lazygit-style.
            one(Char('1'), Command::Focus(crate::app::Focus::Files)),
            one(Char('2'), Command::Focus(crate::app::Focus::Old)),
            one(Char('3'), Command::Focus(crate::app::Focus::New)),
            // Cycle panels, vim-window style.
            ctrl(Char('h'), Command::FocusPrev),
            ctrl(Char('l'), Command::FocusNext),
            one(Char('n'), Command::NextHunk),
            one(Char('p'), Command::PrevHunk),
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
    fn digits_focus_panels() {
        let mut km = Keymap::default_bindings();
        assert_eq!(
            km.feed(plain('1')),
            Resolution::Run(Command::Focus(crate::app::Focus::Files))
        );
        assert_eq!(
            km.feed(plain('3')),
            Resolution::Run(Command::Focus(crate::app::Focus::New))
        );
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
    fn unknown_key_is_none() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('z')), Resolution::None);
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
