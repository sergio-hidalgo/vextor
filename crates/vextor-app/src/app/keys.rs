//! Keyboard shortcuts: their keys, and reading them each frame.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use vextor_core::{Arrange, ToolId};

use super::{Action, VextorApp};

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
pub(super) const CONVERT_TO_PATH: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::P);

// Tools: plain letters (every layout has them).
pub(super) const SELECT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::V);
pub(super) const TEXT_TOOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::T);
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

impl VextorApp {
    /// Keyboard shortcuts, most specific first (⇧⌘Z before ⌘Z). Edit and
    /// Object keys work only while the canvas shows (`editing`), and are
    /// left to a text field that has the focus.
    pub(super) fn shortcuts(ctx: &egui::Context, editing: bool) -> Vec<Action> {
        let mut actions = Self::document_shortcuts(ctx);
        if !editing || ctx.egui_wants_keyboard_input() {
            return actions;
        }
        ctx.input_mut(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Cut => actions.push(Action::Cut),
                    egui::Event::Copy => actions.push(Action::Copy),
                    egui::Event::Paste(_) | egui::Event::PasteImage(_) => {
                        actions.push(Action::Paste);
                    }
                    _ => {}
                }
            }
            let keys = [
                (OUTLINE_TEXT, Action::OutlineText),
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

    /// File, Window, Undo/Redo and View shortcuts.
    fn document_shortcuts(ctx: &egui::Context) -> Vec<Action> {
        let next = Action::CycleTab { forward: true };
        let prev = Action::CycleTab { forward: false };
        ctx.input_mut(|i| {
            [
                (REDO, Action::Redo),
                (UNDO, Action::Undo),
                (NEW, Action::New),
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
    fn letters_pick_tools() {
        assert_eq!(
            actions(Key::T, Modifiers::NONE),
            vec![Action::Tool(ToolId::Text)]
        );
        assert_eq!(
            actions(Key::V, Modifiers::NONE),
            vec![Action::Tool(ToolId::Select)]
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
