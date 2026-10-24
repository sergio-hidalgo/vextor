//! Object ▸ Path Operations (spec 11), from the [`ShapeOp`] table.

use eframe::egui;

use super::Action;
use super::keys::{EXCLUDE, INTERSECT, SUBTRACT, UNION};
use super::menus::why_item;
use crate::session::Session;
use crate::shape_ops::ShapeOp;

/// Object ▸ Path Operations (spec 11): the booleans and Divide, then
/// Offset, Outline Stroke and Simplify, then compound paths.
pub(super) fn shape_menu(ui: &mut egui::Ui, session: Option<&Session>, actions: &mut Vec<Action>) {
    let shortcut = |op: ShapeOp| match op {
        ShapeOp::Union => Some(UNION),
        ShapeOp::Subtract => Some(SUBTRACT),
        ShapeOp::Intersect => Some(INTERSECT),
        ShapeOp::Exclude => Some(EXCLUDE),
        _ => None,
    };
    let groups = [
        &ShapeOp::COMBINE[..],
        &ShapeOp::MODIFY[..],
        &ShapeOp::COMPOUND[..],
    ];
    for (i, group) in groups.into_iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        for &op in group {
            let enabled = session.is_some_and(|s| op.can(s));
            let clicked = match shortcut(op) {
                Some(key) => why_item(ui, enabled, op.title(), key, op.why()),
                None => ui
                    .add_enabled(enabled, egui::Button::new(op.title()))
                    .on_disabled_hover_text(op.why())
                    .clicked(),
            };
            if clicked {
                actions.push(Action::Shape(op));
            }
        }
    }
}
