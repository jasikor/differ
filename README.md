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

**Keys:** `Ctrl-h`/`Ctrl-l` cycle panels (files / old / new) · `j`/`k`
(or arrows) move · `n`/`p` next/prev hunk · `zz`/`zt`/`zb` put the cursor line
at middle/top/bottom · `Ctrl-e`/`Ctrl-y` scroll the view a line (cursor stays) ·
`Ctrl-w o` zoom the focused pane · `PgUp`/`PgDn` page · `Home`/`g` top ·
`R` reload · `q`/`Esc` quit.

## License

GPL-3.0-only — see [LICENSE](LICENSE). Copyleft: if you distribute a modified
version, you must make the source available under the same terms.
