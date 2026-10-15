# differ

A side-by-side diff viewer for the terminal, with **rails** and **connectors**
that visually tie each change together across the two panes.

## The idea

- Two panes: the old file on the left, the new file on the right.
- Hue-coded rails mark the real lines of each change — **removed = red**,
  **added = green**, **changed = blue**.
- Rails sit *inside* the panes, hugging the code, and are joined to the gutter
  by tees (`├` `┤`) and horizontal bridges.
- Removed / added blocks have no rail on the opposite side; instead a
  **two-row gap arrow** marks exactly where the lines were.
- Rounded line style (`─ │ ╭ ╮ ╯ ╰`) throughout.

## Run

```sh
cargo run -p differ -- --repo /path/to/repo   # defaults to the current directory
```

**Keys:** `Ctrl-h`/`Ctrl-l` cycle panels (files / old / new) · `h`/`j`/`k`/`l`
(or arrows) move · `w`/`b`/`e` by word, `W`/`B`/`E` by WORD · `0`/`^`/`$`
line start / first non-blank / end · `]c`/`[c` (or `Ctrl-n`/`Ctrl-p`)
next/prev change · `s` + a
pattern to jump to a visible match (flash-style) · `/` search (literal,
smartcase) with `n`/`N` to repeat · `Space n`/`Space p` next/previous file from
any focus · `gg`/`G` top/bottom ·
`zz`/`zt`/`zb` put the cursor line at middle/top/bottom · `Ctrl-e`/`Ctrl-y`
scroll the view a line (cursor stays) · `Ctrl-z` zoom the focused pane ·
`Space e` show/hide the file panel · `PgUp`/`PgDn` page ·
`R` reload · `q` quit · `Esc` cancels a pending key · `?` full-screen key
help (press a prefix like `Space` to see its options in the status line).

## License

GPL-3.0-only — see [LICENSE](LICENSE). Copyleft: if you distribute a modified
version, you must make the source available under the same terms.
