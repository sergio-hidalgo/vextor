//! The pattern editor (spec 09): which pattern, its name, and its tile's
//! scale, rotation and offset (numbers in the right-hand field column).
//! Patterns are made with Object ▸ Make Pattern.

use eframe::egui;
use vextor_core::{Command, NodeId, Paint, Resource, ResourceId};

use super::layout::{self, Row};
use crate::paint_edit::{self, Edit, Target};
use crate::session::Session;

/// The editor for pattern `id`, painting `target` of `ids`; the command
/// to run, if any. A drag is one undo step.
pub(super) fn editor(
    ui: &mut egui::Ui,
    s: &mut Session,
    ids: &[NodeId],
    target: Target,
    id: ResourceId,
) -> Option<Command> {
    let Some(Resource::Pattern(p)) = s.doc.resources.get(&id).cloned() else {
        return None;
    };
    let (scale, degrees, (dx, dy)) = paint_edit::tile_parts(p.transform);
    let mut cmd = None;
    let mut parts = (scale * 100.0, degrees, dx, dy);
    let mut changed = false;
    if let Some(other) = super::resource_menu(ui, s, target, id, false) {
        let edit = Edit::Paint(Paint::Pattern(other));
        cmd = paint_edit::command(&s.doc, ids, target, &edit);
    }
    let named = super::name_row(ui, s, target, id);
    cmd = cmd.or(named);
    layout::group_gap(ui);
    for (name, value, range, suffix) in [
        ("Scale", &mut parts.0, 1.0..=10_000.0, " %"),
        ("Rotation", &mut parts.1, -360.0..=360.0, "°"),
        ("Offset X", &mut parts.2, -10_000.0..=10_000.0, ""),
        ("Offset Y", &mut parts.3, -10_000.0..=10_000.0, ""),
    ] {
        let row = Row::new(ui, name);
        let r = row.put(ui, row.field2(), |ui| {
            ui.add_sized(
                ui.available_size(),
                egui::DragValue::new(value)
                    .range(range)
                    .suffix(suffix)
                    .max_decimals(2),
            )
        });
        if r.drag_started() {
            s.begin_gesture();
        }
        changed |= r.changed();
        if r.drag_stopped() {
            s.end_gesture();
        }
    }
    if changed && cmd.is_none() {
        let mut edited = p;
        edited.transform = paint_edit::tile_transform(parts.0 / 100.0, parts.1, (parts.2, parts.3));
        cmd = Some(paint_edit::edit_resource(id, Resource::Pattern(edited)));
    }
    cmd
}
