# The app's artwork

Two things live here: the menu bar item's marks, and the app icon.

## Where it comes from

Everything in this directory was **drawn for this repository**. Nothing here is a
vendored third-party asset, so there is no third-party licence and no attribution
obligation to carry: the SVG files are the drawings the rasters were made from,
and the artwork is covered by this repository's MIT licence, like the rest of the
source.

None of it is Apple's. SF Symbols may be named in code and are never copied here:
their files may not be redistributed.

The SVGs are paths in a 16x16 viewBox:

- `mark.svg` is the whirl.
- `mark-paused.svg` is the whirl beside a pause, for `paused=true`.
- `app-icon.svg` is the same whirl in white on a rounded square, on the macOS
  icon grid (an 824x824 square inset 100 px in a 1024 canvas, corner radius
  185.4), over a two-stop blue gradient.

## The files

| file | what it is |
|---|---|
| `mark.svg`, `mark-paused.svg` | the marks, vector source |
| `tray-iconTemplate.png`, `tray-iconTemplate@2x.png` | the running mark, 16x16 and 32x32 |
| `tray-icon-pausedTemplate.png`, `tray-icon-pausedTemplate@2x.png` | the paused mark, 16x16 and 32x32 |
| `app-icon.svg` | the app icon, vector source |
| `app-icon-1024.png` | the app icon's raster source, and what the generator reads |
| `app-icon.icns`, `app-icon.ico`, `app-icon-{128,256,512}.png` | generated, not hand-edited |

The four tray files are **template images**: black plus alpha, with no colour and
no grey in them, which is what lets macOS draw them from the alpha alone and
invert them for a light and a dark menu bar. The suite holds that
(`crates/whirl-ui/src/icon.rs`), and the tray sets them as templates.

## Regenerating

    scripts/make-icons.sh

It reads `app-icon-1024.png` and writes `app-icon.icns` (via `iconutil`), the
`.ico` and the three PNG sizes (via `sips`). Both tools ship with macOS.

The rasters are committed rather than built from the vectors because no macOS
built-in rasterises an SVG with an alpha channel: `qlmanage` renders one onto
white. That is also why the generator does not build the tray marks. The rasters
committed here were rendered from the SVGs with `rsvg-convert` 2.63.2, and
`scripts/make-icons.sh` redoes that when `rsvg-convert` happens to be on the
PATH; without it, the committed PNGs are used as they are.

There is no bundling step in this repository yet, so nothing reads the `.icns`,
the `.ico` or the PNG sizes today: they are here for the step that adds one.
