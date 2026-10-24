//! The Object menu (spec 05 on): group, clip, convert, path, path
//! operations, arrange, align, distribute, flip, artboards.

use eframe::egui;
use vextor_core::Arrange;
use vextor_geom::Align;

use super::keys::{
    ADD_NODE, BACK, BACKWARD, CONVERT_TO_PATH, DELETE, FORWARD, FRONT, GROUP, JOIN, OUTLINE_TEXT,
    TOGGLE_SEGMENT, UNGROUP,
};
use super::menus::why_item;
use super::{Action, VextorApp};
use crate::edit_ops;
use crate::path_ops::PathAction;
use crate::session::Session;

impl VextorApp {
    /// Object: group, arrange, align, distribute, flip.
    pub(super) fn object_menu(
        &mut self,
        ui: &mut egui::Ui,
        can: edit_ops::Can,
        actions: &mut Vec<Action>,
    ) {
        let select_first = "Select an object first";
        if why_item(
            ui,
            can.group,
            "Group",
            GROUP,
            "Select objects in the same group or layer",
        ) {
            actions.push(Action::Group);
        }
        if why_item(ui, can.ungroup, "Ungroup", UNGROUP, "Select a group") {
            actions.push(Action::Ungroup);
        }
        let make_clip = ui
            .add_enabled(can.make_clip, egui::Button::new("Make Clip"))
            .on_hover_text("The topmost selected shape clips the objects under it")
            .on_disabled_hover_text(edit_ops::MAKE_CLIP_HINT);
        if make_clip.clicked() {
            actions.push(Action::MakeClip);
        }
        if ui
            .add_enabled(can.release_clip, egui::Button::new("Release Clip"))
            .on_disabled_hover_text("Select a clip group")
            .clicked()
        {
            actions.push(Action::ReleaseClip);
        }
        ui.separator();
        if why_item(
            ui,
            can.outline,
            "Convert Text to Outlines",
            OUTLINE_TEXT,
            "Select a text",
        ) {
            actions.push(Action::OutlineText);
        }
        if why_item(
            ui,
            can.convert,
            "Convert to Path",
            CONVERT_TO_PATH,
            "Select a rectangle, ellipse, polygon, star or line",
        ) {
            actions.push(Action::ConvertToPath);
        }
        let make_pattern = ui
            .add_enabled(can.edit, egui::Button::new("Make Pattern"))
            .on_hover_text(
                "Turns copies of the selection into a pattern, to pick in Fill or Stroke",
            )
            .on_disabled_hover_text("Select the objects to repeat");
        if make_pattern.clicked() {
            actions.push(Action::MakePattern);
        }
        let session = self.docs.session().filter(|_| self.picker.is_none());
        ui.menu_button("Path", |ui| path_menu(ui, session, actions));
        ui.menu_button("Path Operations", |ui| {
            super::shape_menu::shape_menu(ui, session, actions);
        });
        ui.separator();
        ui.add_enabled_ui(can.edit, |ui| {
            ui.menu_button("Arrange", |ui| arrange_menu(ui, can, actions));
            ui.menu_button("Align", |ui| {
                align_menu(ui, &mut self.align_to_artboard, actions);
            });
        })
        .response
        .on_disabled_hover_text(select_first);
        ui.add_enabled_ui(can.distribute, |ui| {
            ui.menu_button("Distribute", |ui| {
                if ui.button("Horizontally").clicked() {
                    actions.push(Action::Distribute { horizontal: true });
                }
                if ui.button("Vertically").clicked() {
                    actions.push(Action::Distribute { horizontal: false });
                }
            });
        })
        .response
        .on_disabled_hover_text("Select three or more objects");
        ui.separator();
        for (text, horizontal) in [("Flip Horizontal", true), ("Flip Vertical", false)] {
            let button = ui
                .add_enabled(can.edit, egui::Button::new(text))
                .on_disabled_hover_text(select_first);
            if button.clicked() {
                actions.push(Action::Flip { horizontal });
            }
        }
        ui.separator();
        let add = ui
            .add_enabled(
                self.docs.session().is_some(),
                egui::Button::new("Add Artboard…"),
            )
            .on_hover_text("Another artboard, to the right of the others");
        if add.clicked() {
            actions.push(Action::AddArtboard);
        }
    }
}

/// Object ▸ Arrange, each item disabled with the reason (spec 08c).
fn arrange_menu(ui: &mut egui::Ui, can: edit_ops::Can, actions: &mut Vec<Action>) {
    for (how, text, _) in edit_ops::ARRANGE {
        let shortcut = match how {
            Arrange::Front => FRONT,
            Arrange::Forward => FORWARD,
            Arrange::Backward => BACKWARD,
            Arrange::Back => BACK,
        };
        if why_item(ui, can.arrange(how), text, shortcut, can.arrange_why(how)) {
            actions.push(Action::Arrange(how));
        }
    }
}

/// Object ▸ Align: the six lines, then the artboard toggle.
fn align_menu(ui: &mut egui::Ui, to_artboard: &mut bool, actions: &mut Vec<Action>) {
    let items = [
        ("Left", Align::Left),
        ("Centre", Align::Centre),
        ("Right", Align::Right),
        ("Top", Align::Top),
        ("Middle", Align::Middle),
        ("Bottom", Align::Bottom),
    ];
    for (i, (text, how)) in items.into_iter().enumerate() {
        if i == 3 {
            ui.separator();
        }
        if ui.button(text).clicked() {
            actions.push(Action::Align(how));
        }
    }
    ui.separator();
    ui.checkbox(to_artboard, "Align to Artboard")
        .on_hover_text("One selected object always aligns to the artboard");
}

/// Object ▸ Path (spec 07): close, open, join, split, reverse, and the
/// segment and node commands.
fn path_menu(ui: &mut egui::Ui, session: Option<&Session>, actions: &mut Vec<Action>) {
    let shortcut = |a: PathAction| match a {
        PathAction::Join => Some(JOIN),
        PathAction::ToggleSegment => Some(TOGGLE_SEGMENT),
        PathAction::AddNode => Some(ADD_NODE),
        PathAction::DeleteNodes => Some(DELETE),
        _ => None,
    };
    for group in [&PathAction::STRUCTURE[..], &PathAction::EDIT[..]] {
        for &a in group {
            let enabled = session.is_some_and(|s| a.can(s));
            let clicked = match shortcut(a) {
                Some(key) => why_item(ui, enabled, a.title(), key, a.why()),
                None => ui
                    .add_enabled(enabled, egui::Button::new(a.title()))
                    .on_disabled_hover_text(a.why())
                    .clicked(),
            };
            if clicked {
                actions.push(Action::Path(a));
            }
        }
        if group.len() == PathAction::STRUCTURE.len() {
            ui.separator();
        }
    }
}
