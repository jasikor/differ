# mockup — confirmed visual choices

Defaults that are settled (as of this session). Change these only if we decide
to revisit the look.

## Rails

- Rails use the **rounded line style's vertical glyph `│`** and are drawn
  **inside the panels, hugging the code**:
  - **left** rail just after the block's old code (one space clear)
  - **right** rail just before the block's new code (one space clear)
- At the connector's row the rail becomes a **tee** (`├` left, `┤` right;
  heavy/double use their weight), and a horizontal **bridge** carries the line
  from the rail out to the panel edge and across the gutter.
- The gutter (rails, arrows and connector lines) uses one muted shade per
  kind: removed = red, added = green, changed = blue. (Brighter `*_FG` shades
  are still used for the inline text spans.)

## Connector line style

- **Rounded** (`LINES[3]`): `─` `│` with corners `╯ ╭ ╮ ╰`.
- A connector meets a rail at the line **nearest the other end**:
  - removed / added: the rail line nearest the gap arrow
  - changed: the end of each rail nearest the other rail
  So a connector leaves the first/top line when the other end is above, the
  last/bottom line when it is below, and the overlapping line when the spans
  overlap.

## Changed lines

- Left rail `│` over the old rows and right rail `│` over the new rows, in the
  changed (blue) colour, joined by a connector.

## Removed lines

- Removed lines have **no after rail**. The exact gap is marked with a
  **two-row arrow**: top glyph `U+1FB3D` on the line above the gap, bottom
  glyph `U+1FB5A` on the line below.
- When the line below the gap is indented, the connector continues into the new
  panel and the arrow is placed one space **before the first non-space
  character**, so it isn't stranded out at the panel edge.
- The left rail `│` spans the removed rows, joined to the arrow by a tee.

## Added lines

- The mirror of the removed case: added lines have **no before rail**. A
  **two-row arrow** marks the gap in the before panel, placed one space **after
  the last non-space character** of the before text:
  - **top glyph `U+1FB48`** on the line above the gap
  - **bottom glyph `U+1FB65`** on the line below the gap
  - (horizontal mirrors of the removed pair `U+1FB3D`/`U+1FB5A`)
- The added lines get a **right rail** (`│`) in the added (green) colour,
  spanning the added rows and joined to the arrow by a tee.


## Colors / spans

- Removed = red, added = green, changed = blue; faint line tints.
- Spans: removed bold + strike; added underline; changed strike (old) +
  underline (new).

## Layout

- Two side-by-side panes, **no frames**; each has a header line.
- Header: `old` shows the rail name; `new` shows end + line + `d=<delta>`.
- Gutter sits between the panes; help bar is a single line at the bottom.

## Samples & keys

- Compared files: `samples/before.txt` (stable) and `samples/after.txt` (edit
  this one); read at runtime.
- The glyph/colour choices above are now fixed (no more cycling/browser).
- Keys: `j`/`k` (or arrows) scroll, `PgUp`/`PgDn` page, `Home`/`G`/`c` top,
  `,`/`.` shift the right pane, `R` reload, `q`/`Esc` quit.
