//! Key bindings: a key *sequence* → [`Command`], so vim-style multi-key
//! mappings (`gg`, `]c`, …) can be added as data without touching the event
//! loop. The default table aims to read like Neovim.

use ratatui::crossterm::event::KeyCode;

use crate::command::Command;

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
    bindings: Vec<(Vec<KeyCode>, Command)>,
    pending: Vec<KeyCode>,
}

impl Keymap {
    pub fn new(bindings: Vec<(Vec<KeyCode>, Command)>) -> Self {
        Keymap {
            bindings,
            pending: Vec::new(),
        }
    }

    /// The default, Neovim-flavoured bindings.
    pub fn default_bindings() -> Self {
        use KeyCode::*;
        let one = |c: KeyCode, cmd: Command| (vec![c], cmd);
        Keymap::new(vec![
            one(Char('q'), Command::Quit),
            one(Esc, Command::Quit),
            one(Char('j'), Command::NextFile),
            one(Down, Command::NextFile),
            one(Char('k'), Command::PrevFile),
            one(Up, Command::PrevFile),
            one(Char('n'), Command::NextHunk),
            one(Char('p'), Command::PrevHunk),
            one(Char('J'), Command::ScrollDown),
            one(Char('K'), Command::ScrollUp),
            one(PageDown, Command::PageDown),
            one(PageUp, Command::PageUp),
            one(Home, Command::Top),
            one(Char('g'), Command::Top),
            one(Char('.'), Command::OffsetRight),
            one(Char(','), Command::OffsetLeft),
            one(Char('R'), Command::Reload),
        ])
    }

    /// Feed one key. Stateful: it accumulates keys until a binding resolves.
    pub fn feed(&mut self, key: KeyCode) -> Resolution {
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
    pub fn pending(&self) -> &[KeyCode] {
        &self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_key_matches_immediately() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(KeyCode::Char('q')), Resolution::Run(Command::Quit));
        assert!(km.pending().is_empty());
    }

    #[test]
    fn unknown_key_is_none() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(KeyCode::Char('z')), Resolution::None);
    }

    #[test]
    fn sequences_pend_then_run() {
        let mut km = Keymap::new(vec![
            (vec![KeyCode::Char('g'), KeyCode::Char('g')], Command::Top),
            (
                vec![KeyCode::Char('g'), KeyCode::Char('h')],
                Command::PrevFile,
            ),
        ]);
        assert_eq!(km.feed(KeyCode::Char('g')), Resolution::Pending);
        assert_eq!(km.pending(), &[KeyCode::Char('g')]);
        assert_eq!(km.feed(KeyCode::Char('g')), Resolution::Run(Command::Top));
        assert!(km.pending().is_empty());
    }
}
