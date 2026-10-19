# Vextor

A very simple desktop vector editor, written in Rust, for drawing the shapes
of web app components — icons, buttons, badges, cards — and copying clean
SVG, HTML or JSX out of them.

**Status:** the editor window, canvas, document model and `.vextor`
files are in place. You can create, open and save documents, select and
transform objects, and add text labels; the shape and pen tools arrive in
later milestones.

## Requirements

- [rustup](https://rustup.rs). The exact toolchain (Rust 1.99.1 with
  `rustfmt` and `clippy`) is pinned in `rust-toolchain.toml` and installed
  automatically on first build.
- [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for the
  dependency audit: `cargo install cargo-deny --locked`.

## Build, run, check

```sh
cargo run -p vextor-app          # opens the editor (binary name: vextor)
cargo run -p vextor-app --release

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                 # advisories, licenses, bans, sources
```

All four checks must pass before a change is merged.

```sh
cargo bench -p vextor-render     # canvas render timings (criterion), typing in long text
cargo bench -p vextor-geom       # hit-testing, drag-frame and text layout timings
```

Text tests use the bundled Noto Sans fonts in
`crates/vextor-geom/tests/fixtures/fonts/` (SIL OFL 1.1, see the README
there), never the machine's fonts.

## Using the editor

On launch, pick an artboard size: Icon 16 / 24 / 32 / 48, Button 160×48,
Card 320×200, or a custom size — or open a file (Open… and the recent files
are under the sizes). The canvas shows a checkerboard inside transparent
artboards and grey around them.

Several documents can be open at once, one tab each above the canvas (`•`
marks unsaved changes; hover a tab for its file). Each keeps its own undo,
selection and view; copy in one and paste in another. Closing the last one
goes back to the start screen. Quitting asks about each unsaved document in
turn, and after a crash every unsaved document is offered back.

| Action                      | How                                        |
| :-------------------------- | :----------------------------------------- |
| New document                | `⌘N` (File ▸ New…), or `+` after the tabs  |
| Open / open a recent file   | `⌘O` (File ▸ Open…) / File ▸ Open Recent (an open file switches to its tab) |
| Close document              | `⌘W` (File ▸ Close), or the `×` that shows when you hover a tab |
| Rename document             | double-click its tab; `Enter` or click elsewhere keeps it, `Esc` cancels. A saved document's file is renamed in its folder (never over another file) |
| Next / previous document    | `⌥⌘→` / `⌥⌘←`, or `⌃Tab` / `⌃⇧Tab` (Window menu lists them all) |
| Save / save as              | `⌘S` / `⇧⌘S`                               |
| Zoom at the pointer         | pinch, or `⌘`-scroll                       |
| Pan                         | two-finger scroll, space-drag, middle-drag |
| Fit artboard / actual size  | `⌘0` / `⌘1` (View menu)                    |
| Rename the artboard         | double-click its name above it; `Enter` or click elsewhere keeps it, `Esc` cancels (up to 64 characters) |
| Artboard name, background   | Inspector ▸ Artboard, with nothing selected: Name, and Background on (starts white) with a colour; off is transparent |
| Undo / redo                 | `⌘Z` / `⇧⌘Z` (Edit menu shows what)        |

### Selecting and transforming

Tools (left panel): Select (`V`) and Text (`T`).

| Action                              | How                                                    |
| :---------------------------------- | :----------------------------------------------------- |
| Select / add or remove              | click / `⇧`-click (a group is selected whole)          |
| Select by area                      | drag on empty canvas: selects what the box touches     |
| Select all / clear                  | `⌘A` / `Esc`                                           |
| Move                                | drag the selection; arrows nudge 1 px, `⇧`+arrows 10 px |
| Scale                               | drag a handle; `⇧` keeps proportions, `⌥` from the centre (strokes keep their width; text scales its font size and always keeps proportions) |
| Rotate                              | drag just outside any corner (curved-arrow cursor); `⇧` in 15° steps |
| Exact position, size, rotation      | Inspector (right): X, Y, W, H, ° (hover the small icons for what each is) |
| Cut / copy / paste / paste in place | `⌘X` / `⌘C` / `⌘V` (+10 px) / `⇧⌘V`                     |
| Duplicate / delete                  | `⌘D` (+10 px) / `⌫`                                    |
| Group / ungroup                     | `⌘G` / `⇧⌘G`                                           |
| Bring forward / to front            | `⌘↑` / `⇧⌘↑`                                           |
| Send backward / to back             | `⌘↓` / `⇧⌘↓`                                           |
| Align, distribute                   | Inspector ▸ Align icon buttons, or the Object menu (the board icon toggles Align to Artboard) |
| Group / ungroup (buttons)           | Inspector ▸ Group, or `⌘G` / `⇧⌘G`                     |
| Flip                                | Object menu                                            |

### Text

Simple, single-style labels (button captions, badges, headings) in the
fonts installed on your computer. The fonts load in the background when
Vextor starts; until then text is placed but not drawn.

| Action                     | How                                                        |
| :------------------------- | :--------------------------------------------------------- |
| Add a label                | Text tool (`T`), click: "Text" appears selected, type over it |
| Edit a label               | double-click it, or `Enter` with one label selected        |
| Finish editing             | `Esc`, or click outside (one undo step per edit)           |
| While editing              | arrows; `⌥`+arrows by word; `⌘`+arrows to the line ends; `⇧` selects; `⌘A`; `⌘C` / `⌘X` / `⌘V` (plain text); `Enter` new line |
| Font, style, size, spacing | Inspector ▸ Text: searchable font list (recently used first, each name in its own face), the family's real styles, size, line height, letter spacing, alignment |
| Colour, background, border | Inspector ▸ Text: text colour; Background (colour, padding, corner radius); Border (colour, width) |
| Convert to outlines        | Object ▸ Convert Text to Outlines (`⇧⌘O`): a group with the background rect and one shape per letter (ungroup to move letters one by one) |

A label emptied when editing ends is removed. A document keeps a font's
name even when this computer does not have it: the inspector says
"Font missing: X — showing Y", and converting such a label to outlines asks
first, since the outlines would take the shape of the stand-in font. Copy
writes labels into the clipboard SVG as live `<text>` (one `<tspan>` per
line, the frame as a `<rect>`), so a web page needs the same font
installed or loaded; convert to outlines for a font-free shape.

Align lines up two or more objects with each other, or one object with its
artboard (Object ▸ Align ▸ Align to Artboard forces the artboard).
Distribute needs three or more. Hidden and locked objects cannot be
selected. Copy also puts the selection on the system clipboard as SVG
(shapes, groups, text, solid colours, strokes and clips; gradients are
written as their first colour and images are left out until full export
arrives).

Ctrl replaces ⌘ on Windows and Linux.

The app icon comes from `crates/vextor-app/assets/icon.svg` and the
"vextor" wordmark on the new-document screen from `assets/wordmark.svg`
(Poppins semi-bold outlines, SIL OFL 1.1, with the logo as the "o"). After
editing either, regenerate the PNG the app uses:

```sh
cargo test -p vextor-app -- --ignored regenerate_icon_png
cargo test -p vextor-app -- --ignored regenerate_wordmark_png
```

The app always uses its dark theme. Text, icons and accents take their
colours from the palette in `crates/vextor-app/src/theme.rs`.

## Files, autosave and recovery

Documents are saved as `.vextor` files (JSON, `format_version: 2`, which
added text; version-1 files still open and are saved as version 2;
embedded images as base64; fonts are never embedded, only their names). Saving writes a temporary file next to the
document and renames it into place, so a crash never leaves a half-written
file. The title bar shows `•` while there are unsaved changes, and closing
the window, opening a file or starting a new document asks
**Save changes?** first.

Opening refuses, with a one-line message, files over 50 MB, files from an
unknown format version, cut-off or damaged files, and files with more than
20 000 objects.

While a document has unsaved changes, Vextor autosaves a copy every
60 seconds into its own folder (macOS:
`~/Library/Application Support/Vextor/recovery`; the OS data folder on
Windows and Linux), never next to your files. A clean save or quit deletes
the copy. If Vextor crashes or is killed, the next launch offers
**Recover unsaved work?**. File ▸ Open Recent lists the last 10 files; the
list is kept in the same folder (`recent.json`).

Debug builds have a **Debug** menu: sample documents (basic shapes, 500
and 5 000 shapes), an undoable "nudge layer" step and a frame-time readout.
A release build shows it when started with `VEXTOR_DEBUG_MENU=1`:

```sh
VEXTOR_DEBUG_MENU=1 cargo run -p vextor-app --release
```

## Crates

| Crate           | Holds                                                      |
| :-------------- | :--------------------------------------------------------- |
| `vextor-core`   | document model, commands, undo history, `.vextor` bytes (no UI, no file I/O) |
| `vextor-geom`   | shape → Bézier conversion, hit-testing, resizing, text layout (system fonts via `fontdb`, shaping via `harfrust`, glyph outlines via `skrifa`); later snapping, booleans |
| `vextor-svg`    | SVG export and import                                      |
| `vextor-render` | rasterising documents for the canvas (`tiny-skia`), sample documents |
| `vextor-app`    | the desktop app (`eframe`/`egui`), file dialogs (`rfd`), autosave |

Dependencies point downwards only: `app → svg/render → geom → core`.

## Document model

Everything the editor changes lives in `vextor_core::Document`: layers,
groups, shapes (rect, ellipse, polygon, star, line, path), text, images,
gradients, patterns, artboards, grid and export settings. A document is
changed only through `Command`s, applied with `History::apply`, so every
edit is undoable; a command either applies completely or not at all, and
the document is validated after each one. Colours are sRGB + alpha, typed and saved as
hex (`#1e90ff80`); a document can choose to save them as
`{"r":30,"g":144,"b":255,"a":128}` instead.
`vextor_core::to_bytes` / `from_bytes` turn a document into `.vextor` bytes
and back; loading checks size, format version, object count and every
document invariant. `vextor_geom` turns any shape into a `kurbo::BezPath`
or an editable path.

## Conventions

- `unsafe` code is forbidden workspace-wide; clippy `all` is an error,
  `pedantic` a warning (`[workspace.lints]` in `Cargo.toml`).
- Dependency policy lives in `deny.toml`. Ignored advisories are listed
  there with the reason; each is revisited when its parent crate updates.
