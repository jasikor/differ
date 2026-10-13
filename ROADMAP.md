# differ — MVP roadmap

## Goal

A TUI that watches a git repo (typically edited by agents):

- a left panel lists changed / added / deleted files;
- selecting one shows its before/after in the two-pane differ view (the look
  designed in `mockup/`);
- plus stage/unstage, commit and push.

Manual refresh for the MVP. `mockup/` stays as the visual reference.

## Locked decisions

1. **Git backend:** shell out to the system `git` (faithful to ssh,
   credentials, hooks, signing). Swap to a library later only if needed.
2. **Before/after semantics:** mirror `git status`
   - *unstaged*: `index ↔ worktree` (`git show :<path>` vs the file)
   - *staged*: `HEAD ↔ index` (`git show HEAD:<path>` vs `git show :<path>`)
   - added file → before empty; deleted file → after empty.
3. **Repo structure:** Cargo workspace at the root; members `mockup/` and a new
   `differ/` crate.
4. **Layout:** `[ file list | old pane | gutter | new pane ]`; file list
   selected with keys; refresh on demand.

## Status

- Stage 1 (skeleton): **done** — workspace + `differ/` crate + `git::Repo`
  (discover/status/contents) + placeholder UI.
- Stage 2 (read + view): **done** — renderer ported to `differ/src/view/`,
  file list + two-pane diff, selection (`j`/`k`), before/after content,
  `R` refresh.

## Known issues (for the Stage 4 bug pass)

- Busy/overlapping connectors on diffs with many hunks (lanes are limited).
- The old-side rail can overlap very long lines (code that reaches the panel
  edge).

## Stage 1 — Skeleton

- Root `Cargo.toml` workspace (`resolver = "2"`, members `["mockup", "differ"]`).
- New `differ/` bin crate (deps: `ratatui`, `similar`).
- `git.rs`: thin `Command` wrapper (`repo_root`, `status`, `contents`).
- Module stubs; runnable stub UI.

*Deliverable:* builds and runs; prints the detected repo root.
*Why:* unblocks the real data source.

## Stage 2 — Read + view end-to-end

- Parse `git status --porcelain=v2` → file entries (staged + unstaged codes,
  rename handling).
- Compute before/after per the semantics above.
- Port the mockup renderer into `differ/src/view/` as a **pure** component:
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

- Fix the known mockup visual issues.
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
  model.rs     DiffRow, diff_rows(), intra_line()   (from mockup)
  view/        panes.rs, gutter.rs, files.rs, theme.rs
  input.rs     key handling
```

## Technical notes

- Git: run with `current_dir(repo_root)`; capture stdout/stderr; a non-zero
  exit is an error to surface, never a panic.
- Async: `std::thread` + `mpsc`, polled via crossterm `event::poll` timeout.
- Keep the renderer pure. Frozen visual defaults live in `mockup/NOTES.md`:
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
4. The mockup **bug/improvement list** — to fold into Stage 4 (kept as a TODO
   below until collected).

## TODO — mockup bugs / improvements

- (to be collected)
