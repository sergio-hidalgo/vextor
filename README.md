# Vextor

A very simple desktop vector editor, written in Rust, for drawing the shapes
of web app components — icons, buttons, badges, cards — and copying clean
SVG, HTML or JSX out of them.

**Status:** the editor window, canvas, document model and `.vextor`
files are in place. You can create, open, save and view a document;
drawing tools arrive in later milestones.

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
cargo bench -p vextor-render     # canvas render timings (criterion)
```

## Using the editor

On launch, pick an artboard size: Icon 16 / 24 / 32 / 48, Button 160×48,
Card 320×200, or a custom size. The canvas shows a checkerboard inside
transparent artboards and grey around them.

| Action                      | How                                        |
| :-------------------------- | :----------------------------------------- |
| New document                | `⌘N` (File ▸ New…)                         |
| Open / open a recent file   | `⌘O` (File ▸ Open…) / File ▸ Open Recent   |
| Save / save as              | `⌘S` / `⇧⌘S`                               |
| Zoom at the pointer         | pinch, or `⌘`-scroll                       |
| Pan                         | two-finger scroll, space-drag, middle-drag |
| Fit artboard / actual size  | `⌘0` / `⌘1` (View menu)                    |
| Undo / redo                 | `⌘Z` / `⇧⌘Z` (Edit menu shows what)        |

Ctrl replaces ⌘ on Windows and Linux.

The app icon comes from `crates/vextor-app/assets/icon.svg`; after
editing it, regenerate the PNG the OS uses:

```sh
cargo test -p vextor-app -- --ignored regenerate_icon_png
```

## Files, autosave and recovery

Documents are saved as `.vextor` files (JSON, `format_version: 1`;
embedded images as base64). Saving writes a temporary file next to the
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
| `vextor-geom`   | shape → Bézier conversion; later hit-testing, snapping, booleans |
| `vextor-svg`    | SVG export and import                                      |
| `vextor-render` | rasterising documents for the canvas (`tiny-skia`), sample documents |
| `vextor-app`    | the desktop app (`eframe`/`egui`), file dialogs (`rfd`), autosave |

Dependencies point downwards only: `app → svg/render → geom → core`.

## Document model

Everything the editor changes lives in `vextor_core::Document`: layers,
groups, shapes (rect, ellipse, polygon, star, line, path), images,
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
