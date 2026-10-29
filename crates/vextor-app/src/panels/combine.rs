//! Inspector ▸ Path Operations (spec 11): Union, Subtract, Intersect and
//! Exclude as icon buttons, after Align. Tooltip "Name (keys)", disabled
//! with what they need.

use eframe::egui::{self, os::OperatingSystem};

use super::PanelAction;
use crate::app::keys;
use crate::icons::{self, Icon};
use crate::session::Session;
use crate::shape_ops::ShapeOp;

/// Each button: the operation and its icon.
const BUTTONS: [(ShapeOp, Icon); 4] = [
    (ShapeOp::Union, Icon::Union),
    (ShapeOp::Subtract, Icon::Subtract),
    (ShapeOp::Intersect, Icon::Intersect),
    (ShapeOp::Exclude, Icon::Exclude),
];

/// "Union (⌥⌘U)"; just the name where it has no keys (off macOS).
fn tip(os: OperatingSystem, op: ShapeOp) -> String {
    match keys::shape_op(os, op) {
        Some(k) => format!("{} ({})", op.title(), keys::label_for(os, k)),
        None => op.title().to_owned(),
    }
}

/// Inspector ▸ Path Operations: the four booleans as icon buttons.
pub fn inspector_section(ui: &mut egui::Ui, s: &Session, out: &mut Vec<PanelAction>) {
    super::section(ui, "Path Operations", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (op, icon) in &BUTTONS {
                let tooltip = tip(ui.ctx().os(), *op);
                if icons::button(ui, *icon, false, op.can(s), &tooltip, op.why()).clicked() {
                    out.push(PanelAction::Shape(*op));
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_are_the_name_and_the_keys() {
        let tips = |os| -> Vec<String> { BUTTONS.iter().map(|(op, _)| tip(os, *op)).collect() };
        assert_eq!(
            tips(OperatingSystem::Mac),
            [
                "Union (⌥⌘U)",
                "Subtract (⌥⌘S)",
                "Intersect (⌥⌘I)",
                "Exclude (⌥⌘E)"
            ]
        );
        assert_eq!(
            tips(OperatingSystem::Windows),
            ["Union", "Subtract", "Intersect", "Exclude"],
            "no Ctrl+Alt keys: that is AltGr"
        );
    }
}
