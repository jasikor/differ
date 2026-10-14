# differ — MVP roadmap

## Goal

A TUI that watches a git repo (typically edited by agents):

- a left panel lists changed / added / deleted files;
- selecting one shows its before/after in the two-pane differ view;
- plus stage/unstage, commit and push.

Manual refresh for the MVP.

## Locked decisions

1. **Git backend:** shell out to the system `git` (faithful to ssh,
   credentials, hooks, signing). Swap to a library later only if needed.
2. **Before/after semantics:** mirror `git status`
   - *unstaged*: `index ↔ worktree` (`git show :<path>` vs the file)
   - *staged*: `HEAD ↔ index` (`git show HEAD:<path>` vs `git show :<path>`)
   - added file → before empty; deleted file → after empty.
3. **Repo structure:** Cargo workspace at the root with a `differ/` crate.
4. **Layout:** `[ file list | old pane | gutter | new pane ]`; file list
   selected with keys; refresh on demand.

## Status

- Stage 1 (skeleton): **done** — workspace + `differ/` crate + `git::Repo`
  (discover/status/contents) + placeholder UI.
- Stage 2 (read + view): **done** — renderer ported to `differ/src/view/`,
  file list + two-pane diff, selection (`j`/`k`), before/after content,
  `R` refresh.
- Stage 3 (mutations): **deferred** — lazygit already covers stage/commit/push;
  `differ` focuses on *readable diffs*.
- **Overview rulers:** done — a 1-column map per pane (outer edges) with hunk
  cells coloured by kind and a viewport indicator, plus a `%`/line readout.
- **Panel focus + scroll sync:** done — `Ctrl-h`/`Ctrl-l` cycle focus
  (files/old/new),
  `j`/`k` move in the focused region, the panes stay aligned via a line-pair
  map (`old_top`/`new_top`), and scrolling is clamped so you can't scroll past
  EOF. The manual offset (`,`/`.`) was dropped.
- **Vim motions:** done — `gg`/`G` jump to the top/bottom of the focused region
  (first/last line of a pane, first/last file in the list), `Home` was dropped
  in favour of them.
- **Word / line motions:** done — `h`/`j`/`k`/`l` move by column and line,
  `w`/`b`/`e` (and the whitespace-delimited `W`/`B`/`E`) move by word and
  `0`/`^`/`$` go to the line start / first non-blank / end, with a per-pane
  cursor column. Long
  lines pan horizontally to keep the cursor column visible (the code scrolls
  under a fixed number margin, and the in-pane rails/arrows follow it, clamped
  to the pane). Hunk navigation moved from `n`/`p` to Vim-diff `]c`/`[c`.
- **Cursor:** done — each pane has a cursor (the sync anchor); the viewport
  follows it, and the paired pane's cursor tracks it via the line map. Drawn as
  a solid block (active brighter than inactive) at the start of the cursor
  line; the cursor line gets one faint tint, the same in both panes.
- **Cursor placement:** done — `zz`/`zt`/`zb` put the focused pane's cursor
  line at the middle/top/bottom (vim-style), and `Ctrl-e`/`Ctrl-y` scroll the
  view a line while the cursor keeps its line; the panes stay paired.
- **Zoom:** done — `Ctrl-z` shows just the focused diff pane full width (the
  other pane, gutter and its ruler are hidden; the in-pane rails stay); the
  hidden pane drops out of the `Ctrl-h`/`Ctrl-l` cycle, and switching to the
  file list keeps the zoom (the pane stays full width until you toggle it off
  from the zoomed pane).
- **Panel toggle:** done — `Space` is the leader and `Space e` shows/hides the
  file (git) panel; hidden, the diff panes take the full width and the panel
  drops out of the `Ctrl-h`/`Ctrl-l` cycle. Hiding it while focused returns
  focus to the last diff pane; showing it focuses the panel.
- Commands are decoupled from keys (`command.rs` / `keymap.rs`); the keymap is a
  data-driven, sequence-capable table. `Esc` cancels a pending sequence; `q`
  quits.

## Known issues (for a future bug pass)

- Busy/overlapping connectors on diffs with many hunks (lanes are limited).
- Horizontal scroll counts one column per `char`, so tabs (and wide/zero-width
  characters) don't line up with the terminal's own column count.

## Stage 1 — Skeleton

- Root `Cargo.toml` workspace (`resolver = "2"`, members `["differ"]`).
- New `differ/` bin crate (deps: `ratatui`, `similar`).
- `git.rs`: thin `Command` wrapper (`repo_root`, `status`, `contents`).
- Module stubs; runnable stub UI.

*Deliverable:* builds and runs; prints the detected repo root.
*Why:* unblocks the real data source.

## Stage 2 — Read + view end-to-end

- Parse `git status --porcelain=v2` → file entries (staged + unstaged codes,
  rename handling).
- Compute before/after per the semantics above.
- Port the renderer into `differ/src/view/` as a **pure** component:
  `(old_text, new_text, view_opts) → widgets`, with no sample I/O.
- Layout with the file list; `j`/`k` selects a file; the panes render it; `R`
  re-reads.

*Deliverable:* a usable read-only viewer on a real repo.

## Stage 3 — Mutate

- Background worker thread + `mpsc` so network ops don't freeze the UI.
- Stage/unstage the selected file (and stage-all); commit (message input); push.
- Refresh after each op; surface errors/messages in a status area; confirm push.

*Deliverable:* the whole basic workflow.

## Stage 4 — Bug pass

- Fix the known visual issues.
- Edge cases: added/deleted files, missing trailing newline, long lines
  (horizontal scroll), empty / no-change file, binary-ish files, large diffs,
  terminal resize.

*Deliverable:* robust enough for daily use.

## Out of scope (post-MVP)

Auto file-watching, hunk/line-level staging, pull/fetch/branch management,
signing, config/theming, mouse, multi-repo, history browsing, arbitrary refs.

## Architecture

```
differ/src/
  main.rs      entry, terminal setup, event loop
  app.rs       state (repo, files, selection, focus, mode, messages)
  git.rs       shell-out git: status, contents, stage, commit, push
  model.rs     DiffRow, diff_rows(), intra_line()
  view/        panes.rs, gutter.rs, files.rs, theme.rs
  input.rs     key handling
```

## Technical notes

- Git: run with `current_dir(repo_root)`; capture stdout/stderr; a non-zero
  exit is an error to surface, never a panic.
- Async: `std::thread` + `mpsc`, polled via crossterm `event::poll` timeout.
- Keep the renderer pure. Frozen visual defaults:
  rails `│`, rounded lines, removed arrow `U+1FB3D`/`U+1FB5A`, added arrow
  `U+1FB48`/`U+1FB65`.

## Testing

- `git.rs`: temp-repo tests (init, commit, modify/stage, assert status + contents).
- `model.rs`: diff/blocks unit tests.
- `view`: `TestBackend` snapshot tests.

## Open questions

1. Commit message: in-TUI line editor vs `$EDITOR`?
2. v1 stage granularity: selected file only, or also stage-all?
3. Sidebar: fixed width or toggleable?

## TODO — future features

- **Ignore whitespace:** an option (with a toggle key) that treats lines
  differing only in whitespace as unchanged, like `git diff -w` /
  `--ignore-all-space`. Feed a whitespace-insensitive comparison into
  `model::diff_rows` so the tints, hunk offsets and sync all follow the mode.
