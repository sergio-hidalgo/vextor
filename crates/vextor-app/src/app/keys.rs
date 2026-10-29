//! Keyboard shortcuts: their keys, their labels, and reading them each
//! frame. The only place keys and key symbols are written (spec 14c):
//! where macOS and Windows differ, a function of the OS picks the key;
//! Linux follows Windows. UI strings get their keys from [`label`].

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, os::OperatingSystem};
use vextor_core::{Arrange, ToolId};

mod labels;
pub(crate) use labels::{label, label_for, modifier_name, pretend_os};

use super::{Action, VextorApp};
use crate::path_ops::PathAction;
use crate::shape_ops::ShapeOp;

pub(crate) const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
pub(crate) const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub(crate) const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub(crate) const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
pub(crate) const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub(crate) const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
/// Windows and Linux only, besides `Ctrl+Shift+Z`.
pub(crate) const REDO_Y: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
pub(crate) const CLOSE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
/// File ▸ Exit's label on Windows and Linux; the OS handles the key.
pub(crate) const EXIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::F4);
// Documents (spec 05f): ⌥⌘ arrows on macOS (shown in the Window menu),
// Ctrl+PageDown / PageUp elsewhere (Ctrl+Alt+arrows rotate the screen with
// some graphics drivers), and ⌃Tab everywhere. No `[ ]` keys: they need
// Option on Spanish and other layouts.
pub(crate) const NEXT_DOC: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::ArrowRight);
pub(crate) const PREV_DOC: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::ArrowLeft);
pub(crate) const NEXT_DOC_PAGE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::PageDown);
pub(crate) const PREV_DOC_PAGE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::PageUp);
pub(crate) const NEXT_DOC_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
pub(crate) const PREV_DOC_TAB: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Tab);
pub(crate) const FIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);
pub(crate) const ACTUAL_SIZE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);
// Zoom in / out one stop (spec 08b): ⌘+ and ⌘- only; no ⌘=, `=` is not a
// key of its own on every layout.
pub(crate) const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus);
pub(crate) const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus);
/// View ▸ Show Grid (spec 08).
pub(crate) const SHOW_GRID: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Quote);

// Edit and Object. ⌘X / ⌘C / ⌘V arrive as clipboard events, not keys; their
// constants only label the menu items.
pub(crate) const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
pub(crate) const CUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::X);
pub(crate) const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
pub(crate) const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);
pub(crate) const PASTE_IN_PLACE: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::V);
pub(crate) const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::D);
pub(crate) const DELETE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Backspace);
pub(crate) const DELETE_FORWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
pub(crate) const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);
pub(crate) const DESELECT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Escape);
pub(crate) const GROUP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
pub(crate) const UNGROUP: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::G);
// Stacking uses ⌘ + arrows (up = towards the front): they sit in the same
// place on every keyboard layout, unlike `[` / `]` (Option on a Spanish one).
pub(crate) const FORWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowUp);
pub(crate) const BACKWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowDown);
pub(crate) const FRONT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::ArrowUp);
pub(crate) const BACK: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::ArrowDown);

pub(crate) const OUTLINE_TEXT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::O);
// Spec 12: Copy As SVG (system clipboard only) and File ▸ Export….
pub(crate) const COPY_AS_SVG: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::C);
pub(crate) const EXPORT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::E);
// Spec 13: File ▸ Import….
pub(crate) const IMPORT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::I);
pub(crate) const CONVERT_TO_PATH: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::P);

// Tools: plain letters (every layout has them).
pub(crate) const SELECT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::V);
pub(crate) const TEXT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::T);
pub(crate) const PEN_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::P);
pub(crate) const NODE_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::A);
pub(crate) const ZOOM_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Z);
// Path editing (spec 07).
pub(crate) const JOIN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::J);
pub(crate) const TOGGLE_SEGMENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::C);
pub(crate) const ADD_NODE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Plus);
// Path operations (spec 11): ⌥⌘ + letter.
pub(crate) const CMD_ALT: Modifiers = Modifiers::COMMAND.plus(Modifiers::ALT);
pub(crate) const UNION: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::U);
pub(crate) const SUBTRACT: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::S);
pub(crate) const INTERSECT: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::I);
pub(crate) const EXCLUDE: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::E);
/// The shape tools' keys, in `ToolId::SHAPES` order.
pub(crate) const SHAPE_TOOLS: [(KeyboardShortcut, ToolId); 5] = [
    (KeyboardShortcut::new(Modifiers::NONE, Key::R), ToolId::Rect),
    (
        KeyboardShortcut::new(Modifiers::NONE, Key::E),
        ToolId::Ellipse,
    ),
    (
        KeyboardShortcut::new(Modifiers::NONE, Key::G),
        ToolId::Polygon,
    ),
    (KeyboardShortcut::new(Modifiers::NONE, Key::S), ToolId::Star),
    (KeyboardShortcut::new(Modifiers::NONE, Key::L), ToolId::Line),
];

/// Nudge distance, and with Shift, in document px.
pub(crate) const NUDGE: i32 = 1;
pub(crate) const BIG_NUDGE: i32 = 10;

/// A path command's keys (spec 07), if it has any.
pub(crate) fn path_action(os: OperatingSystem, action: PathAction) -> Option<KeyboardShortcut> {
    match action {
        PathAction::Join => Some(JOIN),
        PathAction::ToggleSegment => Some(TOGGLE_SEGMENT),
        PathAction::AddNode => Some(ADD_NODE),
        PathAction::DeleteNodes => Some(delete(os)),
        _ => None,
    }
}

/// An arrange command's keys (spec 08c).
pub(crate) fn arrange(how: Arrange) -> KeyboardShortcut {
    match how {
        Arrange::Front => FRONT,
        Arrange::Forward => FORWARD,
        Arrange::Backward => BACKWARD,
        Arrange::Back => BACK,
    }
}

/// Redo's keys, the menu's label first: `⇧⌘Z` on macOS; `Ctrl+Y` and
/// `Ctrl+Shift+Z` elsewhere.
pub(crate) fn redo(os: OperatingSystem) -> &'static [KeyboardShortcut] {
    if os.is_mac() {
        &[REDO]
    } else {
        &[REDO_Y, REDO]
    }
}

/// Next and previous document, as the Window menu shows them.
pub(crate) fn cycle_docs(os: OperatingSystem) -> [KeyboardShortcut; 2] {
    if os.is_mac() {
        [NEXT_DOC, PREV_DOC]
    } else {
        [NEXT_DOC_PAGE, PREV_DOC_PAGE]
    }
}

/// Delete's label: `⌫` on macOS, `Del` elsewhere; both keys work on both.
pub(crate) fn delete(os: OperatingSystem) -> KeyboardShortcut {
    if os.is_mac() { DELETE } else { DELETE_FORWARD }
}

/// A path operation's keys: `⌥⌘` + letter on macOS; none elsewhere, where
/// `Ctrl+Alt` is `AltGr` and types a character on many layouts (`€`).
pub(crate) fn shape_op(os: OperatingSystem, op: ShapeOp) -> Option<KeyboardShortcut> {
    if !os.is_mac() {
        return None;
    }
    match op {
        ShapeOp::Union => Some(UNION),
        ShapeOp::Subtract => Some(SUBTRACT),
        ShapeOp::Intersect => Some(INTERSECT),
        ShapeOp::Exclude => Some(EXCLUDE),
        _ => None,
    }
}

/// Whether a key event holds `Ctrl+Alt` off macOS: that is `AltGr` on
/// Spanish, German, French and other layouts (`AltGr+1` types `|`), so no
/// shortcut takes it there.
fn is_alt_gr(os: OperatingSystem, event: &egui::Event) -> bool {
    !os.is_mac()
        && matches!(event, egui::Event::Key { modifiers, .. } if modifiers.command && modifiers.alt)
}

/// A pasted bitmap as an action; `None` for an impossible size.
fn paste_image(image: &egui::ColorImage) -> Option<Action> {
    let [w, h] = image.size;
    let rgba = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    Some(Action::PasteImage {
        width: u32::try_from(w).ok()?,
        height: u32::try_from(h).ok()?,
        rgba,
    })
}

impl VextorApp {
    /// Keyboard shortcuts, most specific first (⇧⌘Z before ⌘Z). Edit and
    /// Object keys work only while the canvas shows (`editing`), and are
    /// left to a text field that has the focus.
    pub(super) fn shortcuts(ctx: &egui::Context, editing: bool) -> Vec<Action> {
        // AltGr keys are left for the text they type, after the shortcuts.
        let os = ctx.os();
        let alt_gr: Vec<egui::Event> = ctx.input_mut(|i| {
            let (alt_gr, rest) = std::mem::take(&mut i.events)
                .into_iter()
                .partition(|e| is_alt_gr(os, e));
            i.events = rest;
            alt_gr
        });
        let actions = Self::read_shortcuts(ctx, editing);
        ctx.input_mut(|i| i.events.extend(alt_gr));
        actions
    }

    fn read_shortcuts(ctx: &egui::Context, editing: bool) -> Vec<Action> {
        let edit_keys = editing && !ctx.egui_wants_keyboard_input();
        // ⌥⌘ letters go first: egui's ⌘S would also take ⌥⌘S.
        let mut actions = if edit_keys && ctx.os().is_mac() {
            Self::path_op_shortcuts(ctx)
        } else {
            Vec::new()
        };
        actions.extend(Self::document_shortcuts(ctx));
        if !edit_keys {
            return actions;
        }
        ctx.input_mut(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Cut => actions.push(Action::Cut),
                    egui::Event::Copy => actions.push(Action::Copy),
                    egui::Event::Paste(text) => actions.push(Action::PasteText(text.clone())),
                    egui::Event::PasteImage(image) => actions.extend(paste_image(image)),
                    _ => {}
                }
            }
            let keys = [
                (COPY_AS_SVG, Action::CopyAs(vextor_svg::Format::Svg)),
                (EXPORT, Action::Export),
                (OUTLINE_TEXT, Action::OutlineText),
                (JOIN, Action::Path(PathAction::Join)),
                (TOGGLE_SEGMENT, Action::Path(PathAction::ToggleSegment)),
                (ADD_NODE, Action::Path(PathAction::AddNode)),
                (CONVERT_TO_PATH, Action::ConvertToPath),
                (PASTE_IN_PLACE, Action::PasteInPlace),
                (DUPLICATE, Action::Duplicate),
                (SELECT_ALL, Action::SelectAll),
                (UNGROUP, Action::Ungroup),
                (GROUP, Action::Group),
                (FRONT, Action::Arrange(Arrange::Front)),
                (BACK, Action::Arrange(Arrange::Back)),
                (FORWARD, Action::Arrange(Arrange::Forward)),
                (BACKWARD, Action::Arrange(Arrange::Backward)),
                (DELETE, Action::Delete),
                (DELETE_FORWARD, Action::Delete),
                (DESELECT, Action::Deselect),
                (SELECT_TOOL, Action::Tool(ToolId::Select)),
                (TEXT_TOOL, Action::Tool(ToolId::Text)),
                (PEN_TOOL, Action::Tool(ToolId::Pen)),
                (NODE_TOOL, Action::Tool(ToolId::Node)),
                (ZOOM_TOOL, Action::Tool(ToolId::Zoom)),
            ];
            let tools = SHAPE_TOOLS.map(|(key, tool)| (key, Action::Tool(tool)));
            for (shortcut, action) in keys.into_iter().chain(tools) {
                if i.consume_shortcut(&shortcut) {
                    actions.push(action);
                }
            }
            let arrows = [
                (Key::ArrowLeft, -1, 0),
                (Key::ArrowRight, 1, 0),
                (Key::ArrowUp, 0, -1),
                (Key::ArrowDown, 0, 1),
            ];
            for (key, x, y) in arrows {
                for (mods, step) in [(Modifiers::SHIFT, BIG_NUDGE), (Modifiers::NONE, NUDGE)] {
                    while i.consume_key(mods, key) {
                        actions.push(Action::Nudge {
                            dx: x * step,
                            dy: y * step,
                        });
                    }
                }
            }
        });
        actions
    }

    /// The path operation keys (⌥⌘U / S / I / E). The letter is read from
    /// the physical key too: on macOS Option can turn it into another
    /// character.
    fn path_op_shortcuts(ctx: &egui::Context) -> Vec<Action> {
        let keys = [
            (UNION, ShapeOp::Union),
            (SUBTRACT, ShapeOp::Subtract),
            (INTERSECT, ShapeOp::Intersect),
            (EXCLUDE, ShapeOp::Exclude),
        ];
        let mut actions = Vec::new();
        ctx.input_mut(|i| {
            i.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                if !(modifiers.command && modifiers.alt) || modifiers.shift {
                    return true;
                }
                let hit = keys.iter().find(|(shortcut, _)| {
                    *key == shortcut.logical_key || *physical_key == Some(shortcut.logical_key)
                });
                match hit {
                    Some((_, op)) => {
                        actions.push(Action::Shape(*op));
                        false
                    }
                    None => true,
                }
            });
        });
        actions
    }

    /// File, Window, Undo/Redo and View shortcuts.
    fn document_shortcuts(ctx: &egui::Context) -> Vec<Action> {
        let os = ctx.os();
        let next = Action::CycleTab { forward: true };
        let prev = Action::CycleTab { forward: false };
        let [next_doc, prev_doc] = cycle_docs(os);
        let redo = redo(os).iter().map(|&key| (key, Action::Redo));
        ctx.input_mut(|i| {
            redo.chain([
                (UNDO, Action::Undo),
                (NEW, Action::New),
                (IMPORT, Action::Import),
                (OPEN, Action::Open),
                (SAVE_AS, Action::SaveAs),
                (SAVE, Action::Save),
                (CLOSE, Action::Close),
                (PREV_DOC_TAB, prev.clone()),
                (NEXT_DOC_TAB, next.clone()),
                (prev_doc, prev),
                (next_doc, next),
                (FIT, Action::Fit),
                (ACTUAL_SIZE, Action::ActualSize),
                (ZOOM_IN, Action::Zoom { inward: true }),
                (ZOOM_OUT, Action::Zoom { inward: false }),
                (SHOW_GRID, Action::ToggleGrid),
            ])
            .filter(|(shortcut, _)| i.consume_shortcut(shortcut))
            .map(|(_, action)| action)
            .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Ctrl sets on Windows and Linux: `ctrl` and `command`.
    const WIN_CTRL: Modifiers = Modifiers::CTRL.plus(Modifiers::COMMAND);

    /// The actions one key press produces on macOS while the canvas shows.
    fn actions(key: Key, modifiers: Modifiers) -> Vec<Action> {
        actions_on(OperatingSystem::Mac, key, modifiers)
    }

    /// The actions one key press produces on `os` while the canvas shows.
    fn actions_on(os: OperatingSystem, key: Key, modifiers: Modifiers) -> Vec<Action> {
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        let out = VextorApp::shortcuts(&ctx, true);
        ctx.end_pass().textures_delta.clear();
        out
    }

    #[test]
    fn command_arrows_restack_on_any_layout() {
        let cmd = Modifiers::COMMAND;
        assert_eq!(
            actions(Key::ArrowUp, cmd),
            vec![Action::Arrange(Arrange::Forward)]
        );
        assert_eq!(
            actions(Key::ArrowDown, cmd),
            vec![Action::Arrange(Arrange::Backward)]
        );
        assert_eq!(
            actions(Key::ArrowUp, CMD_SHIFT),
            vec![Action::Arrange(Arrange::Front)]
        );
        assert_eq!(
            actions(Key::ArrowDown, CMD_SHIFT),
            vec![Action::Arrange(Arrange::Back)]
        );
    }

    #[test]
    fn command_quote_shows_the_grid() {
        assert_eq!(
            actions(Key::Quote, Modifiers::COMMAND),
            vec![Action::ToggleGrid]
        );
    }

    #[test]
    fn command_plus_and_minus_zoom_but_not_command_equals() {
        let zin = vec![Action::Zoom { inward: true }];
        assert_eq!(actions(Key::Plus, Modifiers::COMMAND), zin);
        assert_eq!(actions(Key::Equals, Modifiers::COMMAND), vec![]);
        assert_eq!(actions(Key::Plus, CMD_SHIFT), zin, "⇧= types +");
        assert_eq!(
            actions(Key::Minus, Modifiers::COMMAND),
            vec![Action::Zoom { inward: false }]
        );
        assert_eq!(
            actions(Key::Plus, Modifiers::NONE),
            vec![Action::Path(PathAction::AddNode)],
            "plain + still adds a node"
        );
    }

    #[test]
    fn letters_pick_tools() {
        assert_eq!(
            actions(Key::T, Modifiers::NONE),
            vec![Action::Tool(ToolId::Text)]
        );
        assert_eq!(
            actions(Key::V, Modifiers::NONE),
            vec![Action::Tool(ToolId::Select)]
        );
        assert_eq!(
            actions(Key::P, Modifiers::NONE),
            vec![Action::Tool(ToolId::Pen)]
        );
        assert_eq!(
            actions(Key::A, Modifiers::NONE),
            vec![Action::Tool(ToolId::Node)]
        );
        assert_eq!(
            actions(Key::Z, Modifiers::NONE),
            vec![Action::Tool(ToolId::Zoom)]
        );
        assert_eq!(
            actions(Key::Z, Modifiers::COMMAND),
            vec![Action::Undo],
            "⌘Z still undoes"
        );
    }

    #[test]
    fn path_keys_join_toggle_and_add_nodes() {
        assert_eq!(
            actions(Key::J, Modifiers::COMMAND),
            vec![Action::Path(PathAction::Join)]
        );
        assert_eq!(
            actions(Key::C, Modifiers::NONE),
            vec![Action::Path(PathAction::ToggleSegment)]
        );
        assert_eq!(
            actions(Key::Plus, Modifiers::NONE),
            vec![Action::Path(PathAction::AddNode)]
        );
        assert_eq!(
            actions(Key::C, Modifiers::COMMAND),
            vec![],
            "⌘C is a copy event"
        );
    }

    #[test]
    fn letters_pick_shape_tools_and_command_shift_p_converts() {
        let picked: Vec<Vec<Action>> = [Key::R, Key::E, Key::G, Key::S, Key::L]
            .into_iter()
            .map(|k| actions(k, Modifiers::NONE))
            .collect();
        let want: Vec<Vec<Action>> = ToolId::SHAPES
            .into_iter()
            .map(|t| vec![Action::Tool(t)])
            .collect();
        assert_eq!(picked, want);
        assert_eq!(actions(Key::P, CMD_SHIFT), vec![Action::ConvertToPath]);
        assert_eq!(actions(Key::S, Modifiers::COMMAND), vec![Action::Save]);
    }

    #[test]
    fn option_command_letters_run_path_operations_not_save() {
        assert_eq!(
            actions(Key::S, CMD_ALT),
            vec![Action::Shape(ShapeOp::Subtract)]
        );
        assert_eq!(
            actions(Key::U, CMD_ALT),
            vec![Action::Shape(ShapeOp::Union)]
        );
        assert_eq!(
            actions(Key::I, CMD_ALT),
            vec![Action::Shape(ShapeOp::Intersect)]
        );
        assert_eq!(
            actions(Key::E, CMD_ALT),
            vec![Action::Shape(ShapeOp::Exclude)]
        );
        assert_eq!(actions(Key::S, Modifiers::COMMAND), vec![Action::Save]);
    }

    #[test]
    fn path_operation_keys_follow_the_physical_key() {
        // Option turned the letter into another key: the physical key counts.
        let ctx = egui::Context::default();
        ctx.set_os(OperatingSystem::Mac);
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Num0,
                physical_key: Some(Key::U),
                pressed: true,
                repeat: false,
                modifiers: CMD_ALT,
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        let out = VextorApp::shortcuts(&ctx, true);
        ctx.end_pass().textures_delta.clear();
        assert_eq!(out, vec![Action::Shape(ShapeOp::Union)]);
    }

    #[test]
    fn shift_command_i_imports_and_option_command_i_still_intersects() {
        assert_eq!(actions(Key::I, CMD_SHIFT), vec![Action::Import]);
        assert_eq!(
            actions(Key::I, CMD_ALT),
            vec![Action::Shape(ShapeOp::Intersect)]
        );
    }

    #[test]
    fn command_v_carries_the_clipboard_text() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            events: vec![egui::Event::Paste("<svg/>".to_owned())],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        let out = VextorApp::shortcuts(&ctx, true);
        ctx.end_pass().textures_delta.clear();
        assert_eq!(out, vec![Action::PasteText("<svg/>".to_owned())]);
    }

    #[test]
    fn command_v_of_a_bitmap_carries_its_pixels() {
        let ctx = egui::Context::default();
        let image = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[10, 20, 30, 255]);
        let raw = egui::RawInput {
            events: vec![egui::Event::PasteImage(std::sync::Arc::new(image))],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        let out = VextorApp::shortcuts(&ctx, true);
        ctx.end_pass().textures_delta.clear();
        assert_eq!(
            out,
            vec![Action::PasteImage {
                width: 1,
                height: 1,
                rgba: vec![10, 20, 30, 255]
            }]
        );
    }

    #[test]
    fn shift_command_c_copies_svg_and_shift_command_e_exports() {
        assert_eq!(
            actions(Key::C, CMD_SHIFT),
            vec![Action::CopyAs(vextor_svg::Format::Svg)]
        );
        assert_eq!(actions(Key::E, CMD_SHIFT), vec![Action::Export]);
        assert_eq!(
            actions(Key::E, CMD_ALT),
            vec![Action::Shape(ShapeOp::Exclude)],
            "⌥⌘E still excludes"
        );
        assert_eq!(
            actions(Key::E, Modifiers::NONE),
            vec![Action::Tool(ToolId::Ellipse)]
        );
    }

    #[test]
    fn plain_arrows_still_nudge() {
        assert_eq!(
            actions(Key::ArrowUp, Modifiers::NONE),
            vec![Action::Nudge { dx: 0, dy: -1 }]
        );
        assert_eq!(
            actions(Key::ArrowLeft, Modifiers::SHIFT),
            vec![Action::Nudge { dx: -10, dy: 0 }]
        );
    }

    #[test]
    fn document_keys_close_and_switch_tabs() {
        assert_eq!(actions(Key::W, Modifiers::COMMAND), vec![Action::Close]);
        let alt_cmd = Modifiers::COMMAND.plus(Modifiers::ALT);
        let next = Action::CycleTab { forward: true };
        let prev = Action::CycleTab { forward: false };
        assert_eq!(actions(Key::ArrowRight, alt_cmd), vec![next.clone()]);
        assert_eq!(actions(Key::ArrowLeft, alt_cmd), vec![prev.clone()]);
        assert_eq!(actions(Key::Tab, Modifiers::CTRL), vec![next]);
        let ctrl_shift = Modifiers::CTRL.plus(Modifiers::SHIFT);
        assert_eq!(actions(Key::Tab, ctrl_shift), vec![prev]);
    }

    #[test]
    fn edit_keys_are_off_while_the_picker_shows() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        assert_eq!(VextorApp::shortcuts(&ctx, false), vec![]);
        assert!(
            ctx.input(|i| i.key_pressed(Key::Escape)),
            "left for the picker"
        );
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn ctrl_keys_work_on_windows() {
        let win = OperatingSystem::Windows;
        assert_eq!(actions_on(win, Key::S, WIN_CTRL), vec![Action::Save]);
        assert_eq!(actions_on(win, Key::Z, WIN_CTRL), vec![Action::Undo]);
        assert_eq!(actions_on(win, Key::G, WIN_CTRL), vec![Action::Group]);
        let ctrl_shift = WIN_CTRL.plus(Modifiers::SHIFT);
        assert_eq!(actions_on(win, Key::S, ctrl_shift), vec![Action::SaveAs]);
        assert_eq!(
            actions_on(win, Key::ArrowUp, ctrl_shift),
            vec![Action::Arrange(Arrange::Front)]
        );
    }

    #[test]
    fn ctrl_y_redoes_on_windows_only() {
        let win = OperatingSystem::Windows;
        assert_eq!(actions_on(win, Key::Y, WIN_CTRL), vec![Action::Redo]);
        let ctrl_shift = WIN_CTRL.plus(Modifiers::SHIFT);
        assert_eq!(actions_on(win, Key::Z, ctrl_shift), vec![Action::Redo]);
        assert_eq!(actions(Key::Y, Modifiers::COMMAND), vec![], "not ⌘Y");
        assert_eq!(actions(Key::Z, CMD_SHIFT), vec![Action::Redo]);
    }

    #[test]
    fn ctrl_page_down_switches_documents_on_windows() {
        let win = OperatingSystem::Windows;
        let next = Action::CycleTab { forward: true };
        let prev = Action::CycleTab { forward: false };
        assert_eq!(actions_on(win, Key::PageDown, WIN_CTRL), vec![next.clone()]);
        assert_eq!(actions_on(win, Key::PageUp, WIN_CTRL), vec![prev]);
        assert_eq!(actions_on(win, Key::Tab, WIN_CTRL), vec![next]);
        let ctrl_alt = WIN_CTRL.plus(Modifiers::ALT);
        assert_eq!(
            actions_on(win, Key::ArrowRight, ctrl_alt),
            vec![],
            "Ctrl+Alt+arrows rotate the screen on some PCs"
        );
    }

    #[test]
    fn delete_and_backspace_delete_on_both() {
        for os in [OperatingSystem::Mac, OperatingSystem::Windows] {
            for key in [Key::Delete, Key::Backspace] {
                assert_eq!(actions_on(os, key, Modifiers::NONE), vec![Action::Delete]);
            }
        }
    }

    #[test]
    fn alt_gr_types_its_character_on_windows() {
        // AltGr is Ctrl+Alt: AltGr+1 types `|` on a Spanish keyboard,
        // AltGr+E types `€`, AltGr+0 `}` on a German one.
        let alt_gr = WIN_CTRL.plus(Modifiers::ALT);
        let win = OperatingSystem::Windows;
        for key in [Key::Num1, Key::Num0, Key::E, Key::S, Key::U, Key::I] {
            assert_eq!(actions_on(win, key, alt_gr), vec![], "{key:?}");
        }
        let ctx = egui::Context::default();
        ctx.set_os(win);
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Num1,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: alt_gr,
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        assert_eq!(VextorApp::shortcuts(&ctx, true), vec![]);
        assert_eq!(ctx.input(|i| i.events.len()), 1, "left for the text");
        ctx.end_pass().textures_delta.clear();
    }

    /// Every shortcut the app shows, as `os` labels it: the menus' items,
    /// the tooltips and the context menu all take their keys from these.
    fn sheet(os: OperatingSystem) -> String {
        use std::fmt::Write as _;
        let fixed = [
            ("New", NEW),
            ("Open", OPEN),
            ("Import", IMPORT),
            ("Close", CLOSE),
            ("Save", SAVE),
            ("Save As", SAVE_AS),
            ("Export", EXPORT),
            ("Undo", UNDO),
            ("Redo", redo(os)[0]),
            ("Cut", CUT),
            ("Copy", COPY),
            ("Copy As SVG", COPY_AS_SVG),
            ("Paste", PASTE),
            ("Paste in Place", PASTE_IN_PLACE),
            ("Duplicate", DUPLICATE),
            ("Delete", delete(os)),
            ("Select All", SELECT_ALL),
            ("Group", GROUP),
            ("Ungroup", UNGROUP),
            ("Convert to Path", CONVERT_TO_PATH),
            ("Convert Text to Outlines", OUTLINE_TEXT),
            ("Fit Artboard", FIT),
            ("Actual Size", ACTUAL_SIZE),
            ("Zoom In", ZOOM_IN),
            ("Zoom Out", ZOOM_OUT),
            ("Show Grid", SHOW_GRID),
            ("Next Document", cycle_docs(os)[0]),
            ("Previous Document", cycle_docs(os)[1]),
        ];
        let arrange = crate::edit_ops::ARRANGE.map(|(how, text)| (text, arrange(how)));
        let paths = PathAction::STRUCTURE
            .into_iter()
            .chain(PathAction::EDIT)
            .filter_map(|a| Some((a.title(), path_action(os, a)?)));
        let shapes = ShapeOp::COMBINE
            .into_iter()
            .filter_map(|op| Some((op.title(), shape_op(os, op)?)));
        let exit = (!os.is_mac()).then_some(("Exit", EXIT));
        fixed
            .into_iter()
            .chain(exit)
            .chain(arrange)
            .chain(paths)
            .chain(shapes)
            .fold(String::new(), |mut s, (text, key)| {
                let _ = writeln!(s, "{text}: {}", label_for(os, key));
                s
            })
    }

    #[test]
    fn windows_labels_have_no_mac_symbols() {
        for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
            let sheet = sheet(os);
            for symbol in ["⌘", "⌥", "⇧", "⌃", "⌫", "⌦"] {
                assert!(!sheet.contains(symbol), "{os:?}: {symbol} in\n{sheet}");
            }
        }
        let mac = sheet(OperatingSystem::Mac);
        assert!(!mac.contains("Ctrl+"), "{mac}");
    }

    #[test]
    fn shortcut_sheet_on_windows() {
        insta::assert_snapshot!(sheet(OperatingSystem::Windows));
    }

    #[test]
    fn shortcut_sheet_on_mac() {
        insta::assert_snapshot!(sheet(OperatingSystem::Mac));
    }

    #[test]
    fn labels_read_like_the_os() {
        let win = OperatingSystem::Windows;
        let mac = OperatingSystem::Mac;
        assert_eq!(label_for(win, GROUP), "Ctrl+G");
        assert_eq!(label_for(mac, GROUP), "⌘G");
        assert_eq!(label_for(win, UNGROUP), "Ctrl+Shift+G");
        assert_eq!(label_for(mac, UNGROUP), "⇧⌘G");
        assert_eq!(label_for(win, FRONT), "Ctrl+Shift+↑");
        assert_eq!(label_for(mac, FRONT), "⇧⌘↑");
        assert_eq!(label_for(win, delete(win)), "Del");
        assert_eq!(label_for(mac, delete(mac)), "⌫");
        assert_eq!(label_for(win, ADD_NODE), "+");
        assert_eq!(label_for(win, EXIT), "Alt+F4");
    }

    #[test]
    fn modifier_names_read_like_the_os() {
        let ctx = egui::Context::default();
        ctx.set_os(OperatingSystem::Windows);
        assert_eq!(modifier_name(&ctx, Modifiers::ALT), "Alt");
        assert_eq!(modifier_name(&ctx, Modifiers::COMMAND), "Ctrl");
        ctx.set_os(OperatingSystem::Mac);
        assert_eq!(modifier_name(&ctx, Modifiers::ALT), "⌥");
        assert_eq!(modifier_name(&ctx, Modifiers::COMMAND), "⌘");
    }

    #[test]
    fn pretend_os_names_a_platform_or_nothing() {
        assert_eq!(pretend_os(Some("windows")), Some(OperatingSystem::Windows));
        assert_eq!(
            pretend_os(Some(" Windows ")),
            Some(OperatingSystem::Windows)
        );
        assert_eq!(pretend_os(Some("linux")), Some(OperatingSystem::Nix));
        assert_eq!(pretend_os(Some("macos")), Some(OperatingSystem::Mac));
        assert_eq!(pretend_os(Some("amiga")), None);
        assert_eq!(pretend_os(None), None);
    }
}
