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
5. **Refresh is manual, by choice.** `R` re-reads `git status` (the whole file
   list) and reloads the selected file's diff. **No** automatic refresh — no
   filesystem watcher and no polling timer. Deliberate: the app stays idle-quiet
   and predictable (the user does not want auto-refresh).

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
  `w`/`b`/`e` (and the whitespace-delimited `W`/`B`/`E`) move by word, crossing
  line boundaries like vim, and
  `0`/`^`/`$` go to the line start / first non-blank / end, with a per-pane
  cursor column. Long
  lines pan horizontally to keep the cursor column visible (the code scrolls
  under a fixed number margin, and the in-pane rails/arrows follow it, clamped
  to the pane). Hunk navigation moved from `n`/`p` to Vim-diff `]c`/`[c`.
- **Jump (`s`):** done — flash.nvim-faithful (`s` is LazyVim's Flash binding):
  type a pattern of any length (case-insensitive) and labels appear on the
  visible matches **in both panes** (the focused pane first, then the other),
  nearest-first within each, from flash's home-row-first alphabet
  `asdfghjklqwertyuiopzxcvbnm` plus uppercase (single characters only; matches
  past 52 stay unlabelled). Picking a label on the other pane switches focus to
  it. A label is never a pattern continuation (flash's `skip` rule, checked on
  both panes), so you can keep typing to narrow the search — that is how the
  surplus matches stay reachable. Labels draw after the match; `Enter` jumps to
  the nearest, `<BS>` edits the pattern, `Esc` cancels.
- **Search (`/`):** done — `/` searches the focused pane's whole diff (not just
  the visible window) with a literal, smartcase pattern (lowercase =
  case-insensitive, any uppercase = case-sensitive), previewing the first match
  as you type (`incsearch`). `Enter` commits, `n`/`N` step through matches with
  wrapping and scroll them into view, `Esc` restores the origin. Matches are
  tinted, the current one brighter.
- **Direct file comparison:** done — `differ <old> <new>` compares two files
  without git (the file panel is hidden, the paths are shown in the pane
  headers, and `R` re-reads both files). `--repo` and two file arguments are
  mutually exclusive.
- **Comparing revisions:** done — the left column now stacks the changed files
  above a versions list (newest first, all refs, tag/branch in its own column,
  separated by a labelled rule rather than a frame). `Ctrl-h`/`Ctrl-l` move
  between columns and `Ctrl-j`/`Ctrl-k` between the files and versions;
  `Space v` jumps to the versions list. `Enter` compares the highlighted
  version with the working tree, `m` marks a base (tinted, with a marker) so
  `Enter` on another version compares the two, and `Space V` types a spec
  instead (`A..B`, `A...B` from the merge base, `A` against the working tree,
  empty for the worktree view).
  `--from`/`--to` do the same from the command line. `Space e` shows/hides the
  whole left column. The file list, `R` and `Ctrl-o` follow the active
  comparison; pane heads name the revs and the status line carries the range.
  `gd`/`gr` still grep the working tree.
- **File switching:** done — `Space n`/`Space p` select the next/previous file
  from any focus (or with the panel hidden), so you can move through the change
  set without leaving a diff pane; it stops at the ends and keeps the current
  focus. Hunk navigation also has `Ctrl-n`/`Ctrl-p` as an alias for `]c`/`[c`.
- **Cursor:** done — each pane has a cursor (the sync anchor); the viewport
  follows it, and the paired pane's cursor tracks it via the line map. Drawn as
  a solid block (active brighter than inactive) at the start of the cursor
  line; the cursor line gets one faint tint, the same in both panes.
- **Cursor placement:** done — `zz`/`zt`/`zb` put the focused pane's cursor
  line at the middle/top/bottom (vim-style), and `Ctrl-e`/`Ctrl-y` scroll the
  view a line while the cursor keeps its line; the panes stay paired.
- **Zoom:** done — `Ctrl-z` zooms the focused region. A diff pane keeps its
  side and hides the other pane, gutter and its ruler (the in-pane rails
  stay); the left panel drops both panes and takes the screen. Hidden regions
  drop out of the `Ctrl-h`/`Ctrl-l`/`Ctrl-j`/`Ctrl-k` movement, and the zoom
  stays until you toggle it off from the zoomed region (hiding the left panel
  clears a left-panel zoom).
- **Panel toggle:** done — `Space` is the leader and `Space e` shows/hides the
  file (git) panel; hidden, the diff panes take the full width and the panel
  drops out of the `Ctrl-h`/`Ctrl-l` cycle. Hiding it while focused returns
  focus to the last diff pane; showing it focuses the panel.
- **Tail over-scroll:** done — when the focused pane is at its last line, further
  down / `G` frames the end of the row model, so the taller side's trailing
  (unpaired) deleted/inserted lines are visible opposite blanks in the other
  pane (previously you had to switch panes to see a trailing deletion).
- Commands are decoupled from keys (`command.rs` / `keymap.rs`); the keymap is a
  data-driven, sequence-capable table. `Esc` cancels a pending sequence; `q`
  quits.
- **Help & which-key:** done — every command has a short description, so the
  which-key line (a pending prefix's options, shown in the status line) is
  keymap-derived, and `?` opens a full-screen help page of grouped keys. The
  status-line hint list is trimmed to `? help` / `q quit`.

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

Auto file-watching (deliberately *not* wanted — see Locked decision 5),
hunk/line-level staging, pull/fetch/branch management,
signing, config/theming, mouse, multi-repo, per-file history browsing.

## Architecture

```
differ-core/src/
  git.rs       shell-out git: status, comparisons, contents, stage, commit, push
  model.rs     DiffRow, diff_rows(), intra_line(), hunks()
  tree.rs      file list ordering
  symbol.rs    ripgrep + tree-sitter definitions / references

differ/src/    (TUI front end)
  main.rs      entry, terminal setup, event loop
  app.rs       state (repo, files, selection, focus, mode, messages)
  command.rs   commands, decoupled from keys
  keymap.rs    data-driven, sequence-capable key table
  view/        panes.rs, gutter.rs, files.rs, ruler.rs, theme.rs

differ-html/src/  (experimental web front end)
  main.rs      CLI: repo or two files -> one HTML file
  render.rs    rows + block spans -> HTML
  style.css    dark theme, panes, hatched counterpart rows
  rails.js     SVG overlay: rails and connectors, hover spotlight
```

## Graphical renderer (experiment)

`differ-html` renders the diff as a self-contained page: HTML rows for the text
(selection and copy still work) and an SVG overlay, positioned from the laid-out
row rectangles, for the rails and connectors.

What the character grid forced and a canvas does not:

- **Connector lanes** — curves route straight across the gutter, so busy diffs
  no longer share a limited number of lanes.
- **Tabs and wide characters** — the browser lays text out; no manual column
  arithmetic.
- **Gap arrows** — a one-sided block ends in a dashed stub and a hollow dot
  instead of the two-row arrow.
- **Wrapping** — long lines wrap (row heights are equalised across panes) or
  clip with `--no-wrap`; no horizontal panning.
- **Hover** — pointing at a changed row spotlights its rails and dims the rest.

Blocks are grouped by row kind (runs of deleted / inserted / changed rows),
not by `model::hunks()`, so a rail never spans rows it does not own. A run of
deleted lines followed by unrelated inserted lines reads as two one-sided
blocks rather than one misleading link.

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
- **Resizable windows (idea, later):** use `Ctrl-Left`/`Ctrl-Right` to resize
  the file panel (currently the fixed `FILES_W` in `view/mod.rs`), and possibly
  `Ctrl-Up`/`Ctrl-Down` for pane proportions / height. No implementation now —
  just capturing the intent (it also answers the "fixed width or toggleable
  sidebar?" open question).
