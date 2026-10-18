# Vextor

A very simple desktop vector editor, written in Rust, for drawing the shapes
of web app components — icons, buttons, badges, cards — and copying clean
SVG, HTML or JSX out of them.

**Status:** the document model is in place (no editor UI yet). Running the
app opens an empty window; drawing tools arrive in later milestones.

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

## Crates

| Crate           | Holds                                                      |
| :-------------- | :--------------------------------------------------------- |
| `vextor-core`   | document model, commands, undo history (no UI, no I/O)     |
| `vextor-geom`   | shape → Bézier conversion; later hit-testing, snapping, booleans |
| `vextor-svg`    | SVG export and import                                      |
| `vextor-render` | rasterising documents for the canvas                       |
| `vextor-app`    | the desktop app (`eframe`/`egui`)                          |

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
`vextor_geom` turns any shape into a `kurbo::BezPath` or an editable path.

## Conventions

- `unsafe` code is forbidden workspace-wide; clippy `all` is an error,
  `pedantic` a warning (`[workspace.lints]` in `Cargo.toml`).
- Dependency policy lives in `deny.toml`. Ignored advisories are listed
  there with the reason; each is revisited when its parent crate updates.
