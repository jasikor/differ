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
cargo run -p differ -- old.txt new.txt        # compare two files directly
```

In direct file mode there is no file panel; the two paths are shown in the pane
headers, and `R` re-reads both files.

**Keys:** `Ctrl-h`/`Ctrl-l` cycle panels (files / old / new) · `h`/`j`/`k`/`l`
(or arrows) move · `w`/`b`/`e` by word, `W`/`B`/`E` by WORD · `0`/`^`/`$`
line start / first non-blank / end · `]c`/`[c` (or `Ctrl-n`/`Ctrl-p`)
next/prev change · `gd`/`gr` go to definition / list references (ripgrep,
tree-sitter for Rust) ·
`Ctrl-o` back after a symbol jump · `s` + a
pattern to jump to a visible match (flash-style) · `/` search (literal,
smartcase) with `n`/`N` to repeat · `Space n`/`Space p` next/previous file from
any focus · `gg`/`G` top/bottom ·
`zz`/`zt`/`zb` put the cursor line at middle/top/bottom · `Ctrl-e`/`Ctrl-y`
scroll the view a line (cursor stays) · `Ctrl-z` zoom the focused pane ·
`Space e` show/hide the file panel · `PgUp`/`PgDn` page ·
`R` reload · `q` quit · `Esc` cancels a pending key · `?` full-screen key
help (press a prefix like `Space` to see its options in the status line).

## Graphical experiment

`differ-html` renders the same diff as a single self-contained web page. The
rails become SVG curves across the gutter instead of box-drawing characters,
hovering a changed row spotlights its rails, and long lines wrap (or are
clipped with `--no-wrap`). Nothing is tied to a character grid, so connectors
no longer compete for lanes.

```sh
cargo run -p differ-html -- --repo . -o /tmp/differ.html --open
cargo run -p differ-html -- old.txt new.txt -o /tmp/differ.html --open
```

The data layer — diff model, git access, symbol lookup, file ordering — lives
in the `differ-core` crate and is shared by both front ends.

## Versioning

Every merged user-facing feature bumps the minor version (`0.2.0`, `0.3.0`, …);
fix-only changes bump the patch (`0.2.1`). The help screen shows the package
version, short commit hash and commit timestamp. A push to `main` tags the
current version as `vX.Y.Z` automatically when that tag does not exist yet; the
version bump itself stays a manual part of the feature branch.

## License

GPL-3.0-only — see [LICENSE](LICENSE). Copyleft: if you distribute a modified
version, you must make the source available under the same terms.
