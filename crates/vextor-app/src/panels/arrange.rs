//! Inspector ▸ Align, ▸ Arrange (spec 08c) and ▸ Group (spec 05d): the
//! Object menu's align, distribute, arrange, group and ungroup as icon
//! buttons. They hand back a [`PanelAction`] that the app runs exactly
//! like the menu item.

use eframe::egui;
use vextor_core::Arrange;
use vextor_geom::Align;

use crate::edit_ops::{self, Can};
use crate::icons::{self, Icon};

/// What a button in these sections asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAction {
    Align(Align),
    Distribute {
        horizontal: bool,
    },
    Arrange(Arrange),
    Group,
    Ungroup,
    /// Artboards ▸ Add (spec 08): opens the size picker.
    AddArtboard,
}

/// The six align buttons: action, icon, tooltip.
const ALIGN: [(Align, Icon, &str); 6] = [
    (Align::Left, Icon::AlignLeft, "Align left edges"),
    (Align::Centre, Icon::AlignCentre, "Align horizontal centres"),
    (Align::Right, Icon::AlignRight, "Align right edges"),
    (Align::Top, Icon::AlignTop, "Align top edges"),
    (Align::Middle, Icon::AlignMiddle, "Align vertical centres"),
    (Align::Bottom, Icon::AlignBottom, "Align bottom edges"),
];

/// What the Align buttons line up: objects, or path nodes (spec 07).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targets {
    Objects,
    Nodes,
}

/// Align: six align buttons, two distribute buttons and the Align to
/// Artboard toggle.
pub fn align_section(
    ui: &mut egui::Ui,
    can: Can,
    targets: Targets,
    to_artboard: &mut bool,
    out: &mut Vec<PanelAction>,
) {
    super::section(ui, "Align", |ui| {
        align_buttons(ui, can, targets, to_artboard, out);
    });
}

fn align_buttons(
    ui: &mut egui::Ui,
    can: Can,
    targets: Targets,
    to_artboard: &mut bool,
    out: &mut Vec<PanelAction>,
) {
    let (select_first, three) = match targets {
        Targets::Objects => ("Select an object first", "Select three or more objects"),
        Targets::Nodes => ("Select nodes first", "Select three or more nodes"),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, (how, icon, tip)) in ALIGN.into_iter().enumerate() {
            if i == 3 {
                ui.add_space(8.0);
            }
            if icons::button(ui, icon, false, can.edit, tip, select_first).clicked() {
                out.push(PanelAction::Align(how));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (horizontal, icon, tip) in [
            (
                true,
                Icon::DistributeH,
                "Distribute horizontally: equal gaps side by side",
            ),
            (
                false,
                Icon::DistributeV,
                "Distribute vertically: equal gaps one above the other",
            ),
        ] {
            if icons::button(ui, icon, false, can.distribute, tip, three).clicked() {
                out.push(PanelAction::Distribute { horizontal });
            }
        }
        ui.add_space(8.0);
        let tip = "Align to artboard";
        if icons::button(ui, Icon::Artboard, *to_artboard, true, tip, "").clicked() {
            *to_artboard = !*to_artboard;
        }
    });
}

/// Arrange: Bring to Front, Bring Forward, Send Backward, Send to Back,
/// disabled with the reason.
pub fn arrange_section(ui: &mut egui::Ui, can: Can, out: &mut Vec<PanelAction>) {
    super::section(ui, "Arrange", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (how, text, keys) in edit_ops::ARRANGE {
                let icon = match how {
                    Arrange::Front => Icon::ArrangeFront,
                    Arrange::Forward => Icon::ArrangeForward,
                    Arrange::Backward => Icon::ArrangeBackward,
                    Arrange::Back => Icon::ArrangeBack,
                };
                let tip = format!("{text} ({keys})");
                let why = can.arrange_why(how);
                if icons::button(ui, icon, false, can.arrange(how), &tip, why).clicked() {
                    out.push(PanelAction::Arrange(how));
                }
            }
        });
    });
}

/// Group: Group and Ungroup buttons, disabled with the reason.
pub fn group_section(ui: &mut egui::Ui, can: Can, out: &mut Vec<PanelAction>) {
    super::section(ui, "Group", |ui| group_buttons(ui, can, out));
}

fn group_buttons(ui: &mut egui::Ui, can: Can, out: &mut Vec<PanelAction>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let group = icons::button(
            ui,
            Icon::Group,
            false,
            can.group,
            "Group (⌘G): make the selected objects one object",
            "Select objects in the same group or layer",
        );
        if group.clicked() {
            out.push(PanelAction::Group);
        }
        let ungroup = icons::button(
            ui,
            Icon::Ungroup,
            false,
            can.ungroup,
            "Ungroup (⇧⌘G): free the objects inside the selected groups",
            "Select a group",
        );
        if ungroup.clicked() {
            out.push(PanelAction::Ungroup);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_align_button_is_a_distinct_alignment() {
        let mut seen: Vec<Align> = ALIGN.iter().map(|(a, ..)| *a).collect();
        seen.dedup();
        assert_eq!(seen.len(), 6);
        assert!(ALIGN.iter().all(|(_, _, tip)| !tip.is_empty()));
    }

    /// Runs the sections headless and clicks the button at `pos`.
    fn click_at(can: Can, pos: egui::Pos2) -> (Vec<PanelAction>, bool) {
        let ctx = egui::Context::default();
        let mut out = Vec::new();
        let mut to_artboard = false;
        for events in [
            vec![],
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                align_section(ui, can, Targets::Objects, &mut to_artboard, &mut out);
                arrange_section(ui, can, &mut out);
                group_section(ui, can, &mut out);
            })
            .textures_delta
            .clear();
        }
        (out, to_artboard)
    }

    const ALL: Can = Can {
        edit: true,
        paste: false,
        group: true,
        ungroup: true,
        distribute: true,
        outline: false,
        convert: false,
        forward: true,
        backward: true,
    };

    #[test]
    fn buttons_hand_back_their_menu_action() {
        // First button of the first row: align left.
        let (out, _) = click_at(ALL, egui::pos2(20.0, 40.0));
        assert_eq!(out, [PanelAction::Align(Align::Left)]);
    }

    #[test]
    fn disabled_buttons_do_nothing() {
        let none = Can {
            edit: false,
            group: false,
            ungroup: false,
            distribute: false,
            forward: false,
            backward: false,
            ..ALL
        };
        let (out, _) = click_at(none, egui::pos2(20.0, 40.0));
        assert_eq!(out, []);
        let (out, _) = click_at(none, ARRANGE_FRONT);
        assert_eq!(out, []);
    }

    /// The first Arrange button, below the Align section.
    const ARRANGE_FRONT: egui::Pos2 = egui::pos2(20.0, 148.0);

    #[test]
    fn arrange_buttons_hand_back_their_menu_action() {
        let (out, _) = click_at(ALL, ARRANGE_FRONT);
        assert_eq!(out, [PanelAction::Arrange(Arrange::Front)]);
    }
}
