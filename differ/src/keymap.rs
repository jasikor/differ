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

    /// A short human label, e.g. `j`, `Ctrl-n`, `Space`, `PgDn`, `Left`.
    pub fn label(&self) -> String {
        match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(c) if self.ctrl => format!("Ctrl-{c}"),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            other => format!("{other:?}"),
        }
    }
}

/// Join a key sequence into a label: `gg`, `]c`, `Space e`, `Ctrl-n`.
pub fn sequence_label(seq: &[Key]) -> String {
    let mut out = String::new();
    for key in seq {
        let label = key.label();
        if label.chars().count() == 1 {
            out.push_str(&label);
        } else {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(&label);
            out.push(' ');
        }
    }
    out.trim_end().to_string()
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
            // Next/previous file from any focus (leader), and ToggleFiles.
            two(' ', 'e', Command::ToggleFiles),
            two(' ', 'n', Command::NextFile),
            two(' ', 'p', Command::PrevFile),
            // Space v compares revisions (the worktree is the default).
            two(' ', 'v', Command::CompareRevisions),
            // ? opens the full-screen key help.
            one(Char('?'), Command::Help),
            // s jumps to a visible character (flash-style).
            one(Char('s'), Command::StartJump),
            // / searches the focused pane; n/N repeat.
            one(Char('/'), Command::Search),
            one(Char('n'), Command::SearchNext),
            one(Char('N'), Command::SearchPrev),
            // Hunk navigation, vim-diff style.
            two(']', 'c', Command::NextHunk),
            two('[', 'c', Command::PrevHunk),
            ctrl(Char('n'), Command::NextHunk),
            ctrl(Char('p'), Command::PrevHunk),
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
            // Naive symbol navigation (ripgrep-backed).
            two('g', 'd', Command::GotoDefinition),
            two('g', 'r', Command::GotoReferences),
            ctrl(Char('o'), Command::Back),
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

    /// The keys buffered so far (for a pending sequence). Drives the which-key
    /// status line.
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    /// The next key + command for each binding that extends the pending keys.
    pub fn continuations(&self) -> Vec<(Key, Command)> {
        let mut out = Vec::new();
        for (seq, cmd) in &self.bindings {
            if seq.len() > self.pending.len() && seq.starts_with(&self.pending) {
                let next = seq[self.pending.len()];
                if !out.iter().any(|(k, _)| *k == next) {
                    out.push((next, *cmd));
                }
            }
        }
        out
    }

    /// The which-key hint for the pending keys, or empty when none are pending.
    pub fn hint(&self) -> String {
        if self.pending().is_empty() {
            return String::new();
        }
        let prefix = sequence_label(self.pending());
        let items: Vec<String> = self
            .continuations()
            .iter()
            .map(|(key, cmd)| format!("{} {}", key.label(), cmd.describe()))
            .collect();
        if items.is_empty() {
            prefix
        } else {
            format!("{prefix}: {}", items.join("   "))
        }
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
    fn gd_gr_and_ctrl_o_drive_symbol_navigation() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(
            km.feed(plain('d')),
            Resolution::Run(Command::GotoDefinition)
        );
        assert_eq!(km.feed(plain('g')), Resolution::Pending);
        assert_eq!(
            km.feed(plain('r')),
            Resolution::Run(Command::GotoReferences)
        );
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('o'))),
            Resolution::Run(Command::Back)
        );
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
        // `/` searches; n/N repeat. (`p` stays free.)
        assert_eq!(km.feed(plain('/')), Resolution::Run(Command::Search));
        assert_eq!(km.feed(plain('n')), Resolution::Run(Command::SearchNext));
        assert_eq!(km.feed(plain('N')), Resolution::Run(Command::SearchPrev));
        assert_eq!(km.feed(plain('p')), Resolution::None);
    }

    #[test]
    fn s_starts_a_jump() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('s')), Resolution::Run(Command::StartJump));
    }

    #[test]
    fn ctrl_n_p_are_hunks_and_leader_n_p_are_files() {
        let mut km = Keymap::default_bindings();
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('n'))),
            Resolution::Run(Command::NextHunk)
        );
        assert_eq!(
            km.feed(Key::ctrl(KeyCode::Char('p'))),
            Resolution::Run(Command::PrevHunk)
        );
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        assert_eq!(km.feed(plain('n')), Resolution::Run(Command::NextFile));
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        assert_eq!(km.feed(plain('p')), Resolution::Run(Command::PrevFile));
    }

    #[test]
    fn question_opens_help() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain('?')), Resolution::Run(Command::Help));
    }

    #[test]
    fn which_key_lists_the_prefix_continuations() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        let keys: Vec<String> = km.continuations().iter().map(|(k, _)| k.label()).collect();
        for k in ["e", "n", "p", "v"] {
            assert!(keys.contains(&k.to_string()), "missing {k} in {keys:?}");
        }
        assert_eq!(keys.len(), 4);
        let hint = km.hint();
        assert!(hint.starts_with("Space: "), "hint: {hint}");
        assert!(hint.contains("toggle file panel"), "hint: {hint}");
    }

    #[test]
    fn space_v_starts_the_revision_prompt() {
        let mut km = Keymap::default_bindings();
        assert_eq!(km.feed(plain(' ')), Resolution::Pending);
        assert_eq!(
            km.feed(plain('v')),
            Resolution::Run(Command::CompareRevisions)
        );
    }

    #[test]
    fn every_binding_has_keys_and_a_description() {
        let km = Keymap::default_bindings();
        for (seq, cmd) in &km.bindings {
            assert!(!sequence_label(seq).is_empty());
            assert!(!cmd.describe().is_empty(), "{cmd:?} has no description");
        }
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
