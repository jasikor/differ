//! Headless core of `differ`: everything that is not a terminal.
//!
//! The front ends (the TUI and the HTML prototype) only render what lives here:
//! `git` reads the change set, `model` turns a before/after pair into aligned
//! rows, `tree` orders the file list and `symbol` finds definitions and
//! references.

pub mod git;
pub mod model;
pub mod symbol;
pub mod tree;
