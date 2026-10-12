# differ

A side-by-side diff viewer for the terminal, with **rails** and **connectors**
that visually tie each change together across the two panes.

> Status: this repo currently holds [`mockup/`](mockup/) — a small
> [`ratatui`](https://ratatui.rs) sketch used to work out the look (rails, gap
> arrows, colours) before building the real thing.

## The idea

- Two panes: the old file on the left, the new file on the right.
- Hue-coded rails mark the real lines of each change — **removed = red**,
  **added = green**, **changed = blue**.
- Rails sit *inside* the panes, hugging the code, and are joined to the gutter
  by tees (`├` `┤`) and horizontal bridges.
- Removed / added blocks have no rail on the opposite side; instead a
  **two-row gap arrow** marks exactly where the lines were.
- Rounded line style (`─ │ ╭ ╮ ╯ ╰`) throughout.

## Run the mockup

```sh
cargo run --manifest-path mockup/Cargo.toml
```

Edit `mockup/samples/after.txt`, then press `R` in the app to reload without
rebuilding. `mockup/samples/before.txt` stays as the reference.

**Keys:** `j`/`k` (or arrows) scroll · `PgUp`/`PgDn` page · `Home`/`G`/`c` top ·
`,`/`.` shift the right pane · `R` reload · `q`/`Esc` quit.

## Design notes

The settled visual choices live in [`mockup/NOTES.md`](mockup/NOTES.md).

## License

GPL-3.0-only — see [LICENSE](LICENSE). Copyleft: if you distribute a modified
version, you must make the source available under the same terms.
