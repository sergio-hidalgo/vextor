# Vextor

A very simple desktop vector editor, written in Rust, for drawing the shapes
of web app components — icons, buttons, badges, cards — and copying clean
SVG, HTML or JSX out of them.

**Status:** workspace scaffolding only. Running the app opens an empty
window; drawing tools arrive in later milestones.

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
| `vextor-geom`   | hit-testing, snapping, booleans, offset, simplify          |
| `vextor-svg`    | SVG export and import                                      |
| `vextor-render` | rasterising documents for the canvas                       |
| `vextor-app`    | the desktop app (`eframe`/`egui`)                          |

Dependencies point downwards only: `app → svg/render → geom → core`.

## Conventions

- `unsafe` code is forbidden workspace-wide; clippy `all` is an error,
  `pedantic` a warning (`[workspace.lints]` in `Cargo.toml`).
- Dependency policy lives in `deny.toml`. Ignored advisories are listed
  there with the reason; each is revisited when its parent crate updates.
