//! Keyboard shortcuts: their keys, and reading them each frame.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use vextor_core::{Arrange, ToolId};

use super::{Action, VextorApp};
use crate::path_ops::PathAction;
use crate::shape_ops::ShapeOp;

pub(super) const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
pub(super) const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub(super) const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub(super) const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
pub(super) const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub(super) const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
pub(super) const CLOSE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
// Documents (spec 05f): ⌥⌘ arrows (shown in the Window menu) and ⌃Tab.
// No `[ ]` keys: they need Option on Spanish and other layouts.
pub(super) const NEXT_DOC: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::ArrowRight);
pub(super) const PREV_DOC: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::ArrowLeft);
pub(super) const NEXT_DOC_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
pub(super) const PREV_DOC_TAB: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Tab);
pub(super) const FIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);
pub(super) const ACTUAL_SIZE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);
// Zoom in / out one stop (spec 08b): ⌘+ and ⌘- only; no ⌘=, `=` is not a
// key of its own on every layout.
pub(super) const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus);
pub(super) const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus);
/// View ▸ Show Grid (spec 08).
pub(super) const SHOW_GRID: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Quote);

// Edit and Object. ⌘X / ⌘C / ⌘V arrive as clipboard events, not keys; their
// constants only label the menu items.
pub(super) const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
pub(super) const CUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::X);
pub(super) const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
pub(super) const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);
pub(super) const PASTE_IN_PLACE: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::V);
pub(super) const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::D);
pub(super) const DELETE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Backspace);
pub(super) const DELETE_FORWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
pub(super) const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);
pub(super) const DESELECT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Escape);
pub(super) const GROUP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
pub(super) const UNGROUP: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::G);
// Stacking uses ⌘ + arrows (up = towards the front): they sit in the same
// place on every keyboard layout, unlike `[` / `]` (Option on a Spanish one).
pub(super) const FORWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowUp);
pub(super) const BACKWARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowDown);
pub(super) const FRONT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::ArrowUp);
pub(super) const BACK: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::ArrowDown);

pub(super) const OUTLINE_TEXT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::O);
// Spec 12: Copy As SVG (system clipboard only) and File ▸ Export….
pub(super) const COPY_AS_SVG: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::C);
pub(super) const EXPORT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::E);
// Spec 13: File ▸ Import….
pub(super) const IMPORT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::I);
pub(super) const CONVERT_TO_PATH: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::P);

// Tools: plain letters (every layout has them).
pub(super) const SELECT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::V);
pub(super) const TEXT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::T);
pub(super) const PEN_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::P);
pub(super) const NODE_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::A);
pub(super) const ZOOM_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Z);
// Path editing (spec 07).
pub(super) const JOIN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::J);
pub(super) const TOGGLE_SEGMENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::C);
pub(super) const ADD_NODE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Plus);
// Path operations (spec 11): ⌥⌘ + letter.
pub(super) const CMD_ALT: Modifiers = Modifiers::COMMAND.plus(Modifiers::ALT);
pub(super) const UNION: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::U);
pub(super) const SUBTRACT: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::S);
pub(super) const INTERSECT: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::I);
pub(super) const EXCLUDE: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::E);
/// The shape tools' keys, in `ToolId::SHAPES` order.
pub(super) const SHAPE_TOOLS: [(KeyboardShortcut, ToolId); 5] = [
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
pub(super) const NUDGE: i32 = 1;
pub(super) const BIG_NUDGE: i32 = 10;

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
        let edit_keys = editing && !ctx.egui_wants_keyboard_input();
        // ⌥⌘ letters go first: egui's ⌘S would also take ⌥⌘S.
        let mut actions = if edit_keys {
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
        let next = Action::CycleTab { forward: true };
        let prev = Action::CycleTab { forward: false };
        ctx.input_mut(|i| {
            [
                (REDO, Action::Redo),
                (UNDO, Action::Undo),
                (NEW, Action::New),
                (IMPORT, Action::Import),
                (OPEN, Action::Open),
                (SAVE_AS, Action::SaveAs),
                (SAVE, Action::Save),
                (CLOSE, Action::Close),
                (PREV_DOC_TAB, prev.clone()),
                (NEXT_DOC_TAB, next.clone()),
                (PREV_DOC, prev),
                (NEXT_DOC, next),
                (FIT, Action::Fit),
                (ACTUAL_SIZE, Action::ActualSize),
                (ZOOM_IN, Action::Zoom { inward: true }),
                (ZOOM_OUT, Action::Zoom { inward: false }),
                (SHOW_GRID, Action::ToggleGrid),
            ]
            .into_iter()
            .filter(|(shortcut, _)| i.consume_shortcut(shortcut))
            .map(|(_, action)| action)
            .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The actions one key press produces while the canvas shows.
    fn actions(key: Key, modifiers: Modifiers) -> Vec<Action> {
        let ctx = egui::Context::default();
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
}
