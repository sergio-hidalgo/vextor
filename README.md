# Vextor

A very simple desktop vector editor, written in Rust, for drawing the shapes
of web app components — icons, buttons, badges, cards — and copying clean
SVG, HTML or JSX out of them.

**Status:** the editor window, canvas, document model and `.vxtr`
files are in place. You can create, open and save documents, draw
rectangles, ellipses, polygons, stars and lines, select and transform
objects, add text labels, draw paths with the pen and edit their nodes,
combine shapes with path operations (union, subtract, offset…), copy
the code out, bring SVGs and PNG / JPEG / WebP images in, and turn a
placed image into vector shapes (black and white, 3 or 6 colours, or
outlines only) to draw over or keep.

## Requirements

- [rustup](https://rustup.rs). The exact toolchain (Rust 1.99.1 with
  `rustfmt` and `clippy`) is pinned in `rust-toolchain.toml` and installed
  automatically on first build.
- [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for the
  dependency audit: `cargo install cargo-deny --locked`.
- Only for fuzzing: a nightly toolchain and
  [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)
  (`rustup toolchain install nightly --profile minimal`,
  `cargo +nightly install cargo-fuzz --locked`).
- Only for the macOS app bundle: the Xcode command line tools
  (`xcode-select --install`), for `swiftc`.

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

To see the Windows (or Linux) keys and labels on a Mac, start the app
with `VEXTOR_PRETEND_OS=windows cargo run -p vextor-app` (`linux` and
`macos` work too). `Ctrl` then does what `⌘` does, the menus read
`Ctrl+…` and File ▸ Exit appears. macOS keeps some `Ctrl` keys for
itself (`Ctrl+arrows` for Spaces), and `Alt+F4` means nothing there.

```sh
cargo bench -p vextor-render     # canvas render timings (criterion), typing in long text,
                                 # and `--bench big`: open, save, frame, hit-test, snap, undo,
                                 # export and import on a 20 000-object document
cargo bench -p vextor-geom       # hit-testing, drag-frame and text layout timings
cargo bench -p vextor-svg        # SVG export of 1 000 objects (budget 20 ms)
```

```sh
# Fuzzing (nightly; `fuzz/` is outside the workspace). Seed the corpus
# with the test fixtures first, e.g.
# cp crates/vextor-svg/tests/fixtures/import/*.svg fuzz/corpus/svg_import/
cargo +nightly fuzz run svg_import -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
cargo +nightly fuzz run vxtr_load  -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
cargo +nightly fuzz run command_seq -- -max_total_time=600   # random edits stay valid
```

Release builds use full LTO, one codegen unit and strip symbols, so they
take about two minutes but the binary is about 10 MB. Debug builds keep no
debug info for dependencies, which roughly halves `target/`; the fuzz
build folder (`fuzz/target/`, several hundred MB) can be removed with
`rm -rf fuzz/target` when not fuzzing.

### macOS app and document icon

```sh
scripts/bundle-macos.sh             # builds target/bundle/Vextor.app
scripts/bundle-macos.sh --install   # ... copies it to ~/Applications
scripts/bundle-macos.sh --dmg       # ... and a compressed target/bundle/Vextor-<version>.dmg
scripts/size-report.sh              # disk used by the binary, .app, .dmg and app data,
                                    # checked against the size budgets (fails when over)
scripts/check-quicklook.sh          # after --install: Finder's thumbnails of sample documents
                                    # (artwork for saved ones, the icon for damaged ones)
scripts/check-quicklook.sh a.vxtr   # ... or what Finder shows for your own files
```

Size budgets (macOS arm64): binary ≤ 12 MB, `Vextor.app` ≤ 13 MB,
`.dmg` ≤ 6 MB, Vextor's own data folder ≤ 1 MB without backups.

The app declares the `.vxtr` file type ("Vextor Document"), so once it
is installed Finder shows saved documents with Vextor's document icon
(`crates/vextor-app/assets/document-icon.svg`) and the app with its icon.
macOS keeps the icons it has already shown in caches, so refresh them
once after installing: `killall iconservicesagent Dock Finder`. If a
`.vxtr` file still shows the old icon, clear the system's icon store with
`sudo rm -rf /Library/Caches/com.apple.iconservices.store` and restart
the Mac. The bundle's app icon is `icon.svg` full bleed (square to the
edges), which macOS rounds to its own shape. The build is signed ad hoc, for your own Mac only.

Finder also shows each document's artwork, like an Illustrator file: in
icon, list and column view, in the column view's preview pane, and with
the space bar. Every save stores a small picture of the document in the
file (all artboards, 512 px on the longest side, about 10–20 KB); two
Quick Look extensions inside `Vextor.app`
(`crates/vextor-app/macos/QuickLook/`, one Swift file built with `swiftc`)
read that picture back. They never draw the document themselves, run
sandboxed with read-only access to the one file, refuse files over 50 MB,
and show the plain document icon for anything unexpected. Files saved
before this version, files saved while the system fonts were still
loading (when they hold text), and autosave copies have no picture and
show the plain icon until saved again. `--install` registers the
extensions; if Finder still shows an old picture or the plain icon after
a save, refresh Quick Look with `qlmanage -r && qlmanage -r cache`.
`qlmanage -t` can't check this on current macOS (it hangs on types served
by app extensions); `scripts/check-quicklook.sh` asks for thumbnails the
way Finder does and saves them in `target/quicklook-check/thumbs/`.
Building the app needs the Xcode command line tools (`swiftc`).

Opening files from outside Vextor works like File ▸ Open…, whether Vextor
is closed or already running: double-click a `.vxtr` in Finder, select
several and press `⌘↓`, drop one on the Dock icon, or use "Open With ▸
Vextor" (which also takes SVG, PNG, JPEG and WebP). Files named on the
command line open the same way (`vextor a.vxtr b.svg`, at most 20). A file
that is already open switches to its tab; each new one opens in a tab of
its own, the last one active; a file that can't be opened shows the usual
message and the others still open. While a dialog is up, the files wait
until it closes. On macOS this takes a small Apple event hook, the
`vextor-macos` crate: the only place in the workspace where `unsafe` code
is allowed.

SVG export and import are checked with `insta` snapshots in
`crates/vextor-svg/tests/snapshots/`; after a deliberate output change,
review each changed file (`cargo insta review`, or `INSTA_UPDATE=always`
and read the diff) before committing.

Text tests use the bundled Noto Sans fonts in
`crates/vextor-geom/tests/fixtures/fonts/` (SIL OFL 1.1, see the README
there), never the machine's fonts.

## Using the editor

Each time Vextor starts, its launch card shows alone for about two seconds
(not when it is already running), then the editor.

On launch, pick an artboard size: Icon 16 / 24 / 32 / 48, Button 160×48,
Card 320×200, or a custom size — or open a file: Open…, and under it the
last three files you worked on that still exist, each with a snapshot
(click one to open it; a deleted or moved file is left out, a damaged one
shows "No preview"). The snapshots
are drawn in the background each time this screen opens, with the same
checks as opening the file. The canvas shows a checkerboard inside transparent
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
| Zoom in / out one step      | `⌘+` / `⌘-` (View menu), about the selection's centre, or the canvas centre with nothing selected. Steps: 10, 25, 50, 66.67, 100, 150, 200, 300, 400, 800, 1 600, 3 200, 6 400 % |
| Zoom tool                   | `Z` (magnifier in the Tools strip): click zooms in one step, `⌥`-click zooms out, drag a rectangle to zoom to it; `Esc` goes back to Select |
| Zoom bar                    | bottom-right of the canvas: `−` / `+` step out / in; click the percentage for the steps, Fit Artboard and Actual Size, or type a zoom and press `Enter` (10–6 400 %) |
| Rename an artboard          | double-click its name above it; `Enter` or click elsewhere keeps it, `Esc` cancels (up to 64 characters) |
| Move an artboard            | drag its name above it: the objects lying wholly on it move with it (`⌥`-drag moves the artboard alone); it snaps like a moved object (`⌘` frees it). One undo step. Or type X and Y in Inspector ▸ Artboard |
| Pick an artboard            | click its name above it, or its row in Inspector ▸ Artboards (with nothing selected); Inspector ▸ Artboard then shows it |
| Add an artboard             | Object ▸ Add Artboard…, or `+` in Inspector ▸ Artboards: pick a size (the same presets as a new document); it goes to the right of the others |
| Reorder / delete an artboard | the arrow and bin buttons on the picked artboard's row in Inspector ▸ Artboards (the order is the export order; a document keeps at least one artboard; deleting one leaves its objects where they are) |
| Artboard name, position, size, background | Inspector ▸ Artboard, with nothing selected: Name; X and Y move the artboard with the objects on it; W and H resize the artboard about its centre (1–10 000 px; the objects on it stay where they are); Background on (starts white) with a colour, off is transparent |
| Undo / redo                 | `⌘Z` / `⇧⌘Z` (Edit menu shows what)        |

### Grid and snapping

The grid, the snapping options and the grid size are saved with the
document; changing them is not an undo step.

| Action                      | How                                        |
| :-------------------------- | :----------------------------------------- |
| Show / hide the grid        | `⌘'` (View ▸ Show Grid); minor lines fade out when zoomed out |
| Grid size                   | View ▸ Grid Size: size (default 8 px) and subdivisions (default 4, so a line every 2 px) |
| Snapping options            | View ▸ Snap to Grid (off by default), Snap to Objects (other objects' nodes, edges and centres), Snap to Artboard (its edges and centre) |
| Snap freely                 | hold `⌘` while dragging                    |
| Crisp icons                 | View ▸ Pixel Snap: rectangles and lines you draw land on whole px |
| Smart guides                | View ▸ Smart Guides (on): while moving, lines show what the selection lines up with |

Snapping works while drawing shapes, clicking with the pen, dragging nodes
and handles, moving and scaling. A small label names what the point
snapped to (Node, Grid, Edge, Centre, Artboard). When several are within
6 screen px, a node wins over the grid, then an edge, a centre, an
artboard.

### Selecting and transforming

Tools (left panel, one icon each; hover for its name and key): Select
(`V`, solid arrow), Nodes (`A`, hollow arrow), Pen (`P`), Text (`T`), and
the shape tools Rectangle (`R`), Ellipse (`E`), Polygon (`G`), Star (`S`)
and Line (`L`), then Zoom (`Z`).

| Action                              | How                                                    |
| :---------------------------------- | :----------------------------------------------------- |
| Select / add or remove              | click / `⇧`-click (a group is selected whole)          |
| Select by area                      | drag on empty canvas: selects what the box touches     |
| Select all / clear                  | `⌘A` / `Esc`                                           |
| Move                                | drag the selection; arrows nudge 1 px, `⇧`+arrows 10 px |
| Scale                               | drag a handle; `⇧` keeps proportions, `⌥` from the centre (strokes keep their width; text scales its font size and always keeps proportions) |
| Rotate                              | drag just outside any corner (curved-arrow cursor); `⇧` in 15° steps |
| Exact position, size, rotation      | Inspector (right): X, Y, W, H, ° (hover the small icons for what each is) |
| Flip                                | Object ▸ Flip Horizontal / Vertical, or the two flip buttons under H in Inspector ▸ Transform |
| Cut / copy / paste / paste in place | `⌘X` / `⌘C` / `⌘V` (+10 px) / `⇧⌘V`                     |
| Duplicate / delete                  | `⌘D` (+10 px) / `⌫`                                    |
| Group / ungroup                     | `⌘G` / `⇧⌘G`                                           |
| Bring forward / to front            | `⌘↑` / `⇧⌘↑`; Forward goes just above the next object it overlaps (strokes count), skipping ones elsewhere, or one place when nothing overlaps |
| Send backward / to back             | `⌘↓` / `⇧⌘↓`; Backward goes just below the next object it overlaps, the same way |
| Arrange (buttons, menu)             | Inspector ▸ Arrange icon buttons, Object ▸ Arrange, or right-click an object (it gets selected first); disabled when already at the front / back |
| Align, distribute                   | Inspector ▸ Align icon buttons, or the Object menu (the board icon toggles Align to Artboard) |
| Group / ungroup (buttons)           | Inspector ▸ Group, or `⌘G` / `⇧⌘G`                     |
| Flip                                | Object menu                                            |

### Shapes

| Action                       | How                                                        |
| :--------------------------- | :--------------------------------------------------------- |
| Draw a shape                 | pick a shape tool, drag on the canvas; the shape is selected and the Select tool comes back |
| Square, circle, 45° line     | hold `⇧` while dragging                                    |
| Draw from the centre         | hold `⌥` while dragging                                    |
| Exact size                   | click without dragging: a dialog asks for the size (`Enter` creates, `Esc` cancels; `⌥`-click centres it on the click) |
| Polygon sides, star points   | `↑` / `↓` while dragging (3–20), or Inspector ▸ Shape      |
| Star inner radius            | `←` / `→` while dragging, or Inspector ▸ Shape (% of the outer radius) |
| Cancel a drag                | `Esc` while dragging                                       |
| Round a rectangle's corners  | with one rectangle selected, drag a dot inside a corner: all corners round together, `⌥` rounds only that one; or Inspector ▸ Corners (each corner, laid out as they sit, or all; hover an icon for its name) |
| Round polygon / star corners | Inspector ▸ Shape ▸ Rounding                               |
| Convert to path              | Object ▸ Convert to Path (`⇧⌘P`): an editable path that looks exactly the same |

New shapes take the last fill and stroke given to a shape in the document
(black fill, no stroke to begin with); a line gets that stroke, or 1 px
black, and no fill. The corner dots show only when the rectangle is at
least 40 points on screen; zoom in for small ones. Each drag, dialog or
inspector change is one undo step.

### Layers

The Layers panel (bottom of the right-hand panel) lists the layers, groups
and objects, top of the stack first.

| Action                        | How                                                        |
| :---------------------------- | :--------------------------------------------------------- |
| Select an object              | click its row (`⇧`-click adds or removes); selecting on the canvas opens and scrolls to its row |
| Current layer                 | click a layer row: it is highlighted, and new shapes, text, paths and pastes go into it when nothing is selected |
| Show / hide, lock / unlock    | the eye and lock at the left of each row. Hidden objects are not drawn or copied; locked ones cannot be clicked on the canvas |
| Rename                        | double-click the name, type, `Enter` (`Esc` cancels; blank goes back to the default name). Text is listed with a `T` and its first line until renamed. Names are kept as typed and become ids (lowercase, words joined by `-`) in exported SVG |
| Open / close a layer or group | the triangle before its icon                               |
| Reorder, move into            | drag a row: drop on the top or bottom edge of a row to place it above or below, on the middle of a layer or group to move it inside. Objects stay where they are on the canvas |
| Layer opacity                 | the Opacity field over the list, for the current layer (each layer keeps its own) |
| New, duplicate, delete layer  | the `+`, duplicate and bin buttons over the list. Delete takes the layer's contents with it; the last layer stays |
| Clip                          | select objects with a shape on top, Object ▸ Make Clip (or right-click): the shape clips the rest, in a new group. Object ▸ Release Clip undoes the clipping |

Every change is one undo step. No blend modes or layer effects.

### Colour and paint

Select one or more objects; Inspector ▸ Fill, Stroke and Opacity show the
first one and change every selected object that has that paint.

| Action                        | How                                                         |
| :---------------------------- | :---------------------------------------------------------- |
| Fill or stroke kind           | Inspector ▸ Fill / Stroke ▸ Paint: None, Solid colour, Linear gradient, Radial gradient, Pattern, currentColor (takes the CSS `color` of the page, for icons). Stroke ▸ None removes the stroke |
| Colour pad                    | click the colour tile next to the hex value: a saturation/brightness square with hue and transparency strips (also for an artboard's background). One drag in it is one undo step |
| Exact colour                  | the hex field (`#RGB`, `#RRGGBB` or `#RRGGBBAA`, applied when you leave the field; a red note says when it does not read, `Esc` puts the colour back) or the R G B A sliders (0–255). RGB only |
| Pick a colour from the canvas | the pipette button, then click the canvas (what you see is picked, transparency included); `Esc` or the button again cancels |
| Swatches                      | under the colour: click one to apply it, `+` adds the current colour, right-click to rename (type, then Enter) or delete. Fills (and text backgrounds) and strokes (and text borders) keep separate lists. Swatches belong to Vextor, not to a document: every document shows them, and they are kept as soon as they change, saved document or not |
| Gradient stops                | the strip under Gradient: click to add a stop, drag a marker to move it; the selected stop's position (%), colour and bin button below (a gradient keeps two stops) |
| Place a gradient              | with one object selected (Select tool), drag the handles on the canvas: start and end (linear); centre, radius and focal point (radial; `⌥`-drag the centre to pull the focal point out) |
| Spread                        | Pad, Reflect or Repeat: how the colours go on past the ends |
| Share a gradient              | Gradient ▸ pick an existing one; editing it changes every object painted with it (the count shows next to the name) |
| Make a pattern                | select objects, Object ▸ Make Pattern: copies of them, tiled by their bounds (strokes included), become a pattern; then Fill or Stroke ▸ Pattern. Scale, rotation and offset of the tile in the inspector |
| Name or delete a gradient or pattern | under its menu in Fill or Stroke: Name (applied when you leave the field, `Esc` cancels; blank goes back to "Pattern 2"), and the bin button: every object painted with it loses that paint (a stroke or border goes). One undo step each |
| Stroke shape                  | Inspector ▸ Stroke: width, cap, join, miter limit, dashes (Solid, Dashed, Dotted, or Custom lengths like `6 3`) |
| Object opacity                | Inspector ▸ Opacity (0–100 %)                               |

Every paint change, drag or slider move is one undo step; undo also puts
back gradients and patterns. Swatch edits are not undo steps. No blend modes, no colour spaces
other than RGB.

### Pen and paths

The pen draws paths node by node, with straight and curved segments. The
Nodes tool then edits any path: its nodes, handles and segments. Other
shapes are never converted behind your back: Object ▸ Convert to Path
(`⇧⌘P`) first, then edit their nodes.

| Action                        | How                                                        |
| :---------------------------- | :--------------------------------------------------------- |
| Straight / curved node        | Pen (`P`): click / click and drag (symmetric handles); `⌥` while dragging moves only the outgoing handle (a corner) |
| Keep to 45° steps             | hold `⇧` while clicking or dragging a handle (the document's angle step) |
| Close the path                | click the first node (a ring shows on it); drag there to curve the closing segment |
| End the path open             | `Enter` or `Esc` (any other command or tool also ends it first) |
| Remove the last node          | `⌫` while drawing                                          |
| Continue an open path         | with the pen, start on one of its open ends (a ring shows)  |
| Join two paths                | while drawing, click another open path's end; or select two open paths, or two end nodes of one path, and Join (`⌘J`) |
| Add a node                    | pen click on the selected path's segment (a `+` shows), or `+` on a selected segment: the shape does not change |
| Edit nodes                    | Nodes (`A`), or double-click a path: click / `⇧`-click / drag a box to select nodes; drag or arrows to move them (`⇧` 45° steps) |
| Handles                       | drag a handle: a smooth node's other handle follows; `⌥`-drag moves it alone (the node becomes a corner). The selected node's handles are yellow; the grey ones belong to its neighbours, and where they overlap the yellow one is grabbed |
| Node type                     | double-click a node: corner → smooth → symmetric → corner (sharp, no handles); or the Corner / Smooth / Symmetric icons in Inspector ▸ Node |
| Segments                      | click one to select it; `C` toggles straight ↔ curve; drag a segment to bend it through the pointer |
| Delete nodes                  | `⌫`: the path stays joined around them; deleting the last ones deletes the path |
| Round a corner node           | select corner nodes and drag the dot inside one: every selected corner rounds, between straight segments or next to curves (the curve is cut back along its length); or Inspector ▸ Node ▸ Corner radius |
| Close, open, join, split, reverse | Object ▸ Path, right-click on the canvas, or the icon buttons in Inspector ▸ Path (hover for the name) |
| Exact node position, handles  | Inspector ▸ Node: X and Y side by side, then the In and Out handles side by side, each with its size and angle |
| Align / distribute nodes      | with nodes selected, Inspector ▸ Align or Object ▸ Align / Distribute: the same six alignments line the nodes up with each other (one node, or Align to Artboard: with the artboard); Distribute spaces three or more evenly. Handles move with their nodes |
| Leave node editing            | `Esc` (nodes → path → Select tool), or double-click empty canvas |

Each drawn path, drag, key press or inspector change is one undo step.
A new path takes the last style given to a shape, with a 1 px black
stroke when that style has none, so open segments show. The pen stays
active for the next path.

### Path operations

Inspector ▸ Path Operations (Union, Subtract, Intersect, Exclude, after
Align), Object ▸ Path Operations, the keys below, and — with two or more
objects selected — the canvas right-click menu (Union, Subtract,
Intersect, Exclude, Divide, under Group / Ungroup; right-clicking a group
shows Group / Ungroup too) work on the selected objects. The keys are
macOS only (see *Windows and Linux* below). Each is one undo step; when a result would
be empty (two shapes that do not touch, intersected) nothing changes and
the message bar says why. Results stay curves: a circle cut by a square
keeps its arcs.

| Do this                    | How                                                         |
| :------------------------- | :---------------------------------------------------------- |
| Union                      | `⌥⌘U`: one shape covering all the selected objects          |
| Subtract                   | `⌥⌘S`: the objects above cut out of the bottom one          |
| Intersect                  | `⌥⌘I`: only where all of them overlap                       |
| Exclude                    | `⌥⌘E`: only where an odd number of them overlap             |
| Divide                     | a group with one piece per region, each painted like the topmost object over it |
| Offset path                | Object ▸ Path Operations ▸ Offset Path…: distance (negative insets), joins, miter limit; the canvas shows the result as you type, OK keeps it, Cancel / `Esc` takes it back |
| Outline stroke             | the stroke becomes a filled shape in the stroke's paint (caps, joins and dashes as drawn); a filled shape becomes a group of its fill and the outlined stroke |
| Simplify                   | Object ▸ Path Operations ▸ Simplify… (paths only): a tolerance in px; fewer nodes, the same look, shown live with the node count |
| Compound path              | Make Compound Path: the selected shapes become one even-odd path, so overlaps make holes; Release Compound Path splits a path into one path per part |
| Fill rule                  | Inspector ▸ Path ▸ Fill rule: Non-zero (overlaps stay filled) or Even-odd (overlaps make holes) |

The result takes the bottom object's place, style, name and opacity.
Groups take part as everything they show; open paths count as closed.
Text takes part as outlines: Vextor asks first, then converts and
combines in the same undo step. Images cannot take part.

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
| Colour, background, border | the letters: Inspector ▸ Fill and Stroke; the frame: Inspector ▸ Text ▸ Background (any paint, padding, corner radius) and Border (any paint, width, dashes) |
| Convert to outlines        | Object ▸ Convert Text to Outlines (`⇧⌘O`): a group with the background rect and one shape per letter (ungroup to move letters one by one) |

A label emptied when editing ends is removed. A document keeps a font's
name even when this computer does not have it: the inspector says
"Font missing: X — showing Y", and converting such a label to outlines asks
first, since the outlines would take the shape of the stand-in font.
Exported code writes labels as live `<text>` (one `<tspan>` per line, the
frame as a `<rect>`), so a web page needs the same font installed or
loaded; turn on the export option "Text as outlines" (or convert to
outlines) for a font-free shape.

Align lines up two or more objects with each other, or one object with its
artboard (Object ▸ Align ▸ Align to Artboard forces the artboard).
Distribute needs three or more. Hidden and locked objects cannot be
selected. Copy also puts the selection on the system clipboard as SVG,
written like any export (see Code and export).

### Code and export

| Action                     | How                                                        |
| :------------------------- | :--------------------------------------------------------- |
| See the code               | the Code panel at the bottom (View ▸ Code Panel shows or hides it): the live code of the current artboard (the one picked in Inspector ▸ Artboards, else the first) or of the selection. Selected objects are highlighted in it, and it scrolls to them |
| Pick a format              | the panel's menu: SVG, Inline HTML (no `xmlns`, `role="img"`, `aria-label` from the artboard name), JSX (a React component named after the artboard, `{...props}` on the `<svg>`), CSS data URI (`url("data:image/svg+xml,…")` for `background` or `mask`) |
| Copy it                    | the panel's Copy button; or Edit ▸ Copy As ▸ SVG (`⇧⌘C`) / Inline HTML / JSX / CSS data URI for the selection |
| Options                    | the panel's Options menu, or File ▸ Export…: decimals (0–6, default 2), ids (from names / unique `vx-…` prefixed / only where needed: none on objects, numbered ones on gradients, patterns and clips so `url(#…)` still finds them), styling (attributes / `<style>` classes), responsive (no `width`/`height`, default on), main colour as `currentColor`, text as outlines, minify. Saved with the document, not undo steps |
| Export files               | File ▸ Export… (`⇧⌘E`), or right-click a selection ▸ Export Selection… (opens on Selection, SVG): the artboard, the selection or all artboards (one file each, named after the artboard, into a folder you pick; same-named files there are replaced), as SVG or PNG 1× / 2× / 3× (`name.png`, `name@2x.png`…) |

The output is minimal and the same every time: shortest path data,
default values left out, `<defs>` in order of first use, and an artboard's
content moved to its top-left corner. Hidden objects, and objects lying
on another artboard, are left out; a layer becomes a `<g>` only when it
has an opacity or a clip. Document text is always escaped, and nothing
executable is ever written. PNGs are drawn by the canvas renderer, so
they look exactly like the canvas.

### Import

| Action                     | How                                                        |
| :------------------------- | :--------------------------------------------------------- |
| Open an SVG or image       | File ▸ Open… (`⌘O`) and pick an `.svg`, `.png`, `.jpg` / `.jpeg` or `.webp`: it opens as a new, untitled document with one artboard the size of the SVG (or of the image, one pixel per px); Save asks for a `.vxtr` name |
| Import into the document   | File ▸ Import… (`⇧⌘I`): an SVG, PNG, JPEG or WebP lands in the middle of the view, selected, as one undo step ("Import"). Several objects come in as one group named after the file |
| Drag and drop              | drop SVG or image files on the canvas: they land where you drop them (a `.vxtr` opens in a tab; with no document open, an SVG or image opens as a new document) |
| Paste SVG or images        | `⌘V` with SVG code on the clipboard (copied from a browser, an icon site, another editor) imports it as objects ("Paste SVG"); with a picture on it (a screenshot, a browser's Copy Image) it lands as an embedded PNG ("Paste image"); with SVG or image files copied in Finder / Explorer, those files are imported. What Vextor copied itself pastes as before, with names and live shapes |

What comes in: groups stay groups (an id becomes the name), every shape
becomes an editable path, and fills, strokes, dashes, opacity, solid
colours, `currentColor`, linear and radial gradients, patterns, clips and
embedded images are kept. Text becomes a text object when it has one
style, its lines sit one under the other and its font is installed;
otherwise it comes in as outlines. Images are embedded (PNG, JPEG, WebP
only), one image pixel per px when dropped on their own. Filters, masks,
blend modes and external images are left out. When anything was left out
or changed, one line says what, e.g. "Imported 12 objects. Left out: 2
filters, 1 mask. 1 text as outlines (font “Inter” is not installed)."

Importing never reads anything but the file itself: links to other files
or to the web are ignored, and files over 50 MB, with more than 20 000
objects, nested too deeply, or with images over 16 384 × 16 384 pixels are
refused with a one-line message. An SVG Vextor exported imports back to
the same objects, so exporting it again gives the same code.

### Image to vector

| To                         | Do                                                                   |
| :------------------------- | :------------------------------------------------------------------- |
| Turn an image into shapes  | select one image, then Object ▸ Image to Vector…, the image's right-click menu, or Inspector ▸ Image ▸ Image to Vector… |
| Pick what you get          | **Black and white**: black shapes where the image is dark, white left empty. **3 colours** / **6 colours**: the image's main colours; in 3 colours, thin lines and small enclosed spots darker than what is around them (a ring round a nose, a patch inside an ear) are drawn in the darkest colour, while shading keeps its colour. **Outlines only**: black lines along every edge between the image's 6 main colours, each edge once, no fill; the line width follows the image's size (restyle them in the Inspector) |
| Compare with the image     | **Keep the image** (on by default) leaves the image under the shapes, hidden; show it again from Layers |
| Keep or drop the result    | the canvas shows the real result while the dialog is open; OK keeps it as one undo step ("Image to vector"), Cancel or `Esc` takes it back |

The result is a group named after the image ("Traced image" when it has
none) in the image's place, with the same position, size and rotation:
one shape per piece (each separate area of a colour, with its holes),
filled, no stroke, named by its colour. Ungroup, and every piece can be
selected and moved on its own. The biggest colour is at the bottom; each
colour reaches 2 px under the pieces above it, so no gaps show between
colours, and stays whole under what it encloses (a face under its eyes),
so moving a piece away shows nothing hidden beneath it. Colours are
real colours of the image (the most common one in each area), not mixes.
Black and white decides what is dark on the image as seen on a white
page, so a cut-out drawing on a transparent background traces like the
same drawing on white. Transparent pixels are never traced, and specks smaller than about 0.05 % of the
image are merged away. Big images are traced at 1 000 pixels on their
longer side, off the UI thread ("Tracing…"); a 4 000 × 3 000 photo takes
well under a second. A result with more than 2 000 shapes or 20 000
nodes is refused ("Too detailed for vector shapes: try fewer colours").
The same image and mode always give the same shapes. Tracing works on a
copy: changing the image later does not change the shapes.

### Windows and Linux

The tables above show the macOS keys. On Windows and Linux every menu,
tooltip and hint shows that platform's keys instead (`Ctrl+S`,
"Group (Ctrl+G)"): `⌘` is `Ctrl`, `⌥` is `Alt`, `⇧` is `Shift`. These
differ:

| Action                         | macOS                       | Windows and Linux                         |
| :----------------------------- | :-------------------------- | :---------------------------------------- |
| Redo                           | `⇧⌘Z`                       | `Ctrl+Y` or `Ctrl+Shift+Z`                |
| Next / previous document       | `⌥⌘→` / `⌥⌘←`, `⌃Tab` / `⌃⇧Tab` | `Ctrl+PageDown` / `Ctrl+PageUp`, `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Delete                         | `⌫` (`Del` works too)       | `Del` (`Backspace` works too)             |
| Union / Subtract / Intersect / Exclude | `⌥⌘U` / `⌥⌘S` / `⌥⌘I` / `⌥⌘E` | no keys: Object ▸ Path Operations, the Inspector or the right-click menu |
| Quit                           | `⌘Q` (Vextor menu)          | File ▸ Exit, or `Alt+F4`                  |

`Ctrl+Alt` is `AltGr` on Spanish, German, French and many other layouts
(`AltGr+E` types `€`), so no shortcut uses it there: an `AltGr` key
types its character and nothing else.

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

Documents are saved as `.vxtr` files: compact JSON, gzip-compressed,
`format_version: 5` (version 2 added text, version 3 the "Text as
outlines" export option, version 4 compact files that leave default
fields out and store each distinct embedded image once, version 5 a
preview picture for Finder). Files from versions 1–4, and uncompressed
`.vxtr` JSON, still open and are saved as version 5; `gunzip -c file.vxtr` shows the JSON. Numbers are saved
exactly, never rounded; images are base64; fonts are never embedded, only
their names. A 20 000-object document takes about 330 KB (it took 21 MB
as version 3).

Saving runs in the background, so the editor never stalls on a big file:
the document is checked first (a document that breaks a rule is never
written, and the message says why), written to a temporary file next to
the target, flushed to disk, read back and compared with what was saved,
and only then renamed over the old file. If any step fails, the old file
is left as it was and a one-line message names the file and the reason.
Exports are written the same way. The title bar shows `•` while there
are unsaved changes, and closing the window, opening a file or starting a
new document asks **Save changes?** first.

Before a save or an export first overwrites an existing file in a session,
Vextor copies that file, byte for byte and checked, to
`~/Library/Application Support/Vextor/backups` (the OS data folder on
Windows and Linux); if the copy fails, the file is not overwritten.
Opening a file makes no backup. **File ▸ Restore from Backup…** lists the
current file's backups (age and size) and opens one as a new, unsaved
document; it never overwrites anything. The same dialog turns backups off
and sets how many are kept: 5 per file and 100 MB in all by default,
oldest removed first, also at startup.

Opening refuses, with a one-line message, files over 50 MB, files from an
unknown format version, cut-off or damaged files, and files with more than
20 000 objects. Opening an `.svg`, `.png`, `.jpg` / `.jpeg` or `.webp` imports it into
a new document (see Import).

While a document has unsaved changes, Vextor autosaves a copy every
60 seconds into its own folder (macOS:
`~/Library/Application Support/Vextor/recovery`; the OS data folder on
Windows and Linux), never next to your files. A clean save or quit deletes
the copy. If Vextor crashes or is killed, the next launch offers
**Recover unsaved work?**. File ▸ Open Recent lists the last 10 files; the
list is kept in the same folder (`recent.json`), and so are the swatches
(`swatches.json`; up to 256, read only up to 1 MB, an unreadable file
starts an empty library). If Vextor itself fails, it writes the latest
unsaved work of every open document (kept every 2 seconds while you edit)
to the same folder before it closes, and the next launch offers it.

## Crates

| Crate           | Holds                                                      |
| :-------------- | :--------------------------------------------------------- |
| `vextor-core`   | document model, commands, undo history, `.vxtr` bytes (no UI, no file I/O), FNV-1a content hashes |
| `vextor-geom`   | shape → Bézier conversion, hit-testing (with a grid index built once per change, so a pointer move on 20 000 objects takes microseconds), resizing, path editing (segment hit-tests, split, bend, joins), text layout (system fonts via `fontdb`, shaping via `harfrust`, glyph outlines via `skrifa`), snapping, where a gradient lands on an object, path operations (booleans on polygons via `i_overlay`, curves recovered from the inputs; offset, outline stroke and simplify via `kurbo`), image tracing (Otsu threshold or k-means colours, pixel-edge outlines, corner detection and curve fitting) |
| `vextor-svg`    | SVG / HTML / JSX / CSS export; SVG import (`usvg`, external resources off, XML bounded before parsing) and PNG / JPEG / WebP import (format and size from the header, `imagesize`) |
| `vextor-render` | rasterising documents for the canvas (`tiny-skia`: solid colours, gradients, patterns, dashes, clips, opacity; images decoded by `image` with size limits, and scaled down for tracing), sample documents |
| `vextor-app`    | the desktop app (`eframe`/`egui` on OpenGL via `glow`), file dialogs (`rfd`), background saving with checks (gzip via `flate2`), autosave, backups, files named on the command line |
| `vextor-macos`  | macOS only: the "open documents" Apple event (Finder, Dock, "Open With") via `objc2`; the one crate allowed `unsafe`, each block with a `// SAFETY:` note |

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
`vextor_core::to_bytes` / `from_bytes` turn a document into `.vxtr` bytes
and back; loading checks size, format version, object count and every
document invariant. `vextor_geom` turns any shape into a `kurbo::BezPath`
or an editable path.

## Conventions

- `unsafe` code is forbidden workspace-wide; clippy `all` is an error,
  `pedantic` a warning (`[workspace.lints]` in `Cargo.toml`).
- Dependency policy lives in `deny.toml`. Ignored advisories are listed
  there with the reason; each is revisited when its parent crate updates.
