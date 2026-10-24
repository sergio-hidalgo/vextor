//! Inspector ▸ Path Operations (spec 11): Union, Subtract, Intersect and
//! Exclude as icon buttons, after Align. Tooltip "Name (keys)", disabled
//! with what they need.

use eframe::egui;

use super::PanelAction;
use crate::icons::{self, Icon};
use crate::session::Session;
use crate::shape_ops::ShapeOp;

/// Each button: the operation, its icon and its keys.
const BUTTONS: [(ShapeOp, Icon, &str); 4] = [
    (ShapeOp::Union, Icon::Union, "⌥⌘U"),
    (ShapeOp::Subtract, Icon::Subtract, "⌥⌘S"),
    (ShapeOp::Intersect, Icon::Intersect, "⌥⌘I"),
    (ShapeOp::Exclude, Icon::Exclude, "⌥⌘E"),
];

/// "Union (⌥⌘U)"; just the name without keys.
fn tip(op: ShapeOp, keys: &str) -> String {
    if keys.is_empty() {
        op.title().to_owned()
    } else {
        format!("{} ({keys})", op.title())
    }
}

/// Inspector ▸ Path Operations: the four booleans as icon buttons.
pub fn inspector_section(ui: &mut egui::Ui, s: &Session, out: &mut Vec<PanelAction>) {
    super::section(ui, "Path Operations", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (op, icon, keys) in &BUTTONS {
                let tooltip = tip(*op, keys);
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
        let tips: Vec<String> = BUTTONS.iter().map(|(op, _, keys)| tip(*op, keys)).collect();
        assert_eq!(
            tips,
            [
                "Union (⌥⌘U)",
                "Subtract (⌥⌘S)",
                "Intersect (⌥⌘I)",
                "Exclude (⌥⌘E)"
            ]
        );
    }
}
