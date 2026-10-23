//! Layers (spec 10): the tree of layers, groups and objects, top of the
//! stack first. Each row: eye, lock, disclosure, kind icon, name
//! (double-click to rename) and a dot when selected. Rows drag to reorder
//! or into a layer or group. Over the list: the current layer's opacity
//! and New / Duplicate / Delete layer. The rules live in `crate::layers`.

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};
use vextor_core::NodeId;

use crate::icons::{self, Icon};
use crate::layers::{self, Drop, Row};
use crate::session::Session;
use crate::theme;

/// How much larger than the rest of the app the panel's text is, in
/// points, so the tree reads at a glance.
const TEXT_BOOST: f32 = 1.0;
/// Height of a row, in points.
const ROW_HEIGHT: f32 = 26.0;
/// Width of the eye and lock columns.
const TOGGLE_WIDTH: f32 = 24.0;
/// Side of the eye and lock icons.
const TOGGLE_ICON: f32 = icons::SIZE;
/// Indent per tree level.
const INDENT: f32 = 14.0;
/// Width of the disclosure triangle's column.
const DISCLOSURE: f32 = 14.0;
/// Side of a row's kind icon.
const KIND_ICON: f32 = 15.0;
/// Space for the selection dot at the right.
const DOT_SPACE: f32 = 16.0;
/// Width of the accent bar at the left of the current layer's row.
const CURRENT_BAR: f32 = 3.0;

/// What a drag in the list carries: the rows it moves.
#[derive(Clone, Debug)]
struct Dragged(Vec<NodeId>);

/// The opacity field and layer buttons (fixed), then the rows (scrolled).
/// Returns a message when an edit was refused.
pub fn show(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    layers::reveal(s);
    for style in [egui::TextStyle::Body, egui::TextStyle::Button] {
        if let Some(font) = ui.style_mut().text_styles.get_mut(&style) {
            font.size += TEXT_BOOST;
        }
    }
    let mut message = toolbar(ui, s);
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("layers-scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in layers::rows(s) {
                if let Some(e) = row_ui(ui, s, &row) {
                    message = Some(e);
                }
            }
        });
    message
}

/// Opacity of the current layer, then New, Duplicate and Delete layer.
fn toolbar(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let mut message = None;
    let layer = layers::current_layer(s)?;
    let opacity = s.doc.nodes.get(&layer).map_or(1.0, |n| n.opacity);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Opacity").color(theme::GREY_300))
            .on_hover_text("Opacity of the current layer");
        let mut percent = opacity * 100.0;
        let r = ui
            .add_sized(
                super::FIELD_SIZE,
                egui::DragValue::new(&mut percent)
                    .range(0.0..=100.0)
                    .max_decimals(0)
                    .suffix(" %"),
            )
            .on_hover_text("Opacity of the current layer");
        if r.drag_started() {
            s.begin_gesture();
        }
        if r.changed() {
            let cmd = layers::set_opacity(&s.doc, layer, percent / 100.0);
            message = layers::apply(s, cmd).err();
        }
        if r.drag_stopped() {
            s.end_gesture();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            super::right_margin(ui, |ui| {
                ui.horizontal(|ui| {
                    let last = s.doc.layers.len() <= 1;
                    let buttons = [
                        (Icon::Add, true, "New layer", ""),
                        (Icon::Duplicate, true, "Duplicate layer", ""),
                        (
                            Icon::Delete,
                            !last,
                            "Delete layer and its contents",
                            "The last layer cannot be deleted",
                        ),
                    ];
                    for (i, (icon, enabled, tip, why)) in buttons.into_iter().enumerate() {
                        if icons::tight_button(ui, icon, enabled, tip, why).clicked() {
                            let done = match i {
                                0 => layers::new_layer(s),
                                1 => layers::duplicate_layer(s),
                                _ => layers::delete_layer(s),
                            };
                            message = done.err().or(message.take());
                        }
                    }
                });
            });
        });
    });
    message
}

/// One row; returns a message when an edit was refused.
fn row_ui(ui: &mut egui::Ui, s: &mut Session, row: &Row) -> Option<String> {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    if row.selected {
        painter.rect_filled(rect, 2.0, visuals.selection.bg_fill);
    } else if row.current {
        // Clearly marked, but quieter than a selected row.
        painter.rect_filled(rect, 2.0, visuals.widgets.inactive.bg_fill);
        let bar = Rect::from_min_size(rect.min, Vec2::new(CURRENT_BAR, rect.height()));
        painter.rect_filled(bar, 1.0, theme::LOGO_AMBER);
    } else if response.hovered() {
        painter.rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
    }
    if s.layers_panel.scroll_to == Some(row.id) {
        response.scroll_to_me(None);
        s.layers_panel.scroll_to = None;
    }

    let dim = row.inherited || !row.visible;
    let x = rect.left();
    let column = |i: f32, w: f32| {
        Rect::from_min_size(Pos2::new(x + i, rect.top()), Vec2::new(w, ROW_HEIGHT))
    };
    let mut message = toggles(ui, s, row, &column);
    #[allow(clippy::cast_precision_loss)] // tree depth ≤ 64
    let indent = 2.0 * TOGGLE_WIDTH + 4.0 + INDENT * row.depth as f32;
    if row.container {
        let r = column(indent, DISCLOSURE);
        let hit = ui.interact(r, ui.scope_id().with((row.id, "open")), Sense::click());
        disclosure(&painter, r, row.open, hit.hovered());
        if hit.clicked() {
            s.layers_panel.toggle(row.id);
        }
    }
    let icon_x = indent + DISCLOSURE + 2.0;
    let colour = if dim {
        theme::GREY_500
    } else {
        theme::GREY_300
    };
    let icon_rect = Rect::from_center_size(
        Pos2::new(x + icon_x + KIND_ICON / 2.0, rect.center().y),
        Vec2::splat(KIND_ICON),
    );
    icons::paint(&painter, icon_rect, row.icon, colour);
    let name_rect = Rect::from_min_max(
        Pos2::new(x + icon_x + KIND_ICON + 6.0, rect.top()),
        Pos2::new(rect.right() - DOT_SPACE, rect.bottom()),
    );
    name(ui, s, row, name_rect, &painter, dim);
    if row.selected {
        painter.circle_filled(
            Pos2::new(rect.right() - DOT_SPACE / 2.0, rect.center().y),
            3.0,
            theme::YELLOW,
        );
    }

    let on_name = response
        .interact_pointer_pos()
        .is_some_and(|p| name_rect.contains(p));
    if response.double_clicked() && on_name {
        layers::start_rename(s, row.id);
    } else if response.clicked() {
        let add = ui.input(|i| i.modifiers.shift);
        layers::click(s, row.id, add);
    }
    if response.drag_started() {
        response.dnd_set_drag_payload(Dragged(layers::dragged(s, row.id)));
    }
    if let Some(e) = drop_target(ui, s, row, &response, rect) {
        message = Some(e);
    }
    if row.clip {
        response.on_hover_text("Clip shape: clips the objects below it in its group");
    }
    message
}

/// The eye and lock of a row, in fixed columns so they line up at every
/// depth. Returns a message when the edit was refused.
fn toggles(
    ui: &mut egui::Ui,
    s: &mut Session,
    row: &Row,
    column: &dyn Fn(f32, f32) -> Rect,
) -> Option<String> {
    let mut message = None;
    let (icon, look, tip) = if row.visible {
        (Icon::Eye, Look::Shown, "Hide")
    } else {
        (Icon::EyeOff, Look::Faint, "Show")
    };
    if toggle(
        ui,
        column(0.0, TOGGLE_WIDTH),
        icon,
        look,
        tip,
        (row.id, "eye"),
    ) {
        message = layers::apply(s, layers::toggle_visible(&s.doc, row.id)).err();
    }
    let (icon, look, tip) = if row.locked {
        (Icon::Lock, Look::Shown, "Unlock")
    } else {
        (Icon::Unlock, Look::Quiet, "Lock")
    };
    if toggle(
        ui,
        column(TOGGLE_WIDTH, TOGGLE_WIDTH),
        icon,
        look,
        tip,
        (row.id, "lock"),
    ) {
        message = layers::apply(s, layers::toggle_locked(&s.doc, row.id)).err();
    }
    message
}

/// The name, or the rename field while renaming: Enter or a click
/// elsewhere keeps the typed name, `Esc` the old one.
fn name(
    ui: &mut egui::Ui,
    s: &mut Session,
    row: &Row,
    rect: Rect,
    painter: &egui::Painter,
    dim: bool,
) {
    if let Some((id, text)) = s.layers_panel.renaming.as_mut()
        && *id == row.id
    {
        let edit = egui::TextEdit::singleline(text)
            .char_limit(vextor_core::MAX_NODE_NAME_CHARS)
            .desired_width(rect.width());
        let r = ui.put(rect.shrink2(Vec2::new(0.0, 1.0)), edit);
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        if r.lost_focus() {
            let typed = text.clone();
            s.layers_panel.renaming = None;
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                let cmd = layers::rename(&s.doc, row.id, &typed);
                // A refused name leaves the old one; nothing to report.
                let _ = layers::apply(s, cmd);
            }
        }
        return;
    }
    let colour = match (dim, row.named) {
        (true, _) => theme::GREY_500,
        (false, true) => theme::GREY_100,
        (false, false) => theme::GREY_300,
    };
    let mut text = egui::RichText::new(&row.label).color(colour);
    if row.depth == 0 {
        text = text.strong();
    }
    if row.clip {
        text = text.underline();
    }
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width(),
        egui::TextStyle::Body,
    );
    let pos = Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0);
    painter.galley(pos, galley, colour);
}

/// An eye or lock toggle in `rect`; true when clicked. `faint` draws it
/// grey (a hidden eye); `quiet` hides it until hovered (an open lock).
fn toggle(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: Icon,
    look: Look,
    tooltip: &str,
    salt: (NodeId, &str),
) -> bool {
    let r = ui
        .interact(rect, ui.scope_id().with(salt), Sense::click())
        .on_hover_text(tooltip);
    let colour = match look {
        _ if r.hovered() => theme::WHITE,
        Look::Shown => theme::GREY_300,
        Look::Faint => theme::GREY_500,
        Look::Quiet => return r.clicked(),
    };
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(TOGGLE_ICON));
    icons::paint(ui.painter(), icon_rect, icon, colour);
    r.clicked()
}

/// How an eye or lock toggle is drawn at rest.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Shown,
    Faint,
    /// Only on hover.
    Quiet,
}

/// A right-pointing (closed) or down-pointing (open) triangle.
fn disclosure(painter: &egui::Painter, rect: Rect, open: bool, hovered: bool) {
    let c = rect.center();
    let colour = if hovered {
        theme::WHITE
    } else {
        theme::GREY_400
    };
    let points = if open {
        vec![
            c + Vec2::new(-4.0, -2.0),
            c + Vec2::new(4.0, -2.0),
            c + Vec2::new(0.0, 3.0),
        ]
    } else {
        vec![
            c + Vec2::new(-2.0, -4.0),
            c + Vec2::new(3.0, 0.0),
            c + Vec2::new(-2.0, 4.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(points, colour, Stroke::NONE));
}

/// While rows are dragged over `row`: marks where they would land (a
/// line above or below, a box around a layer or group they would go
/// into) and drops them on release. Returns a message when the drop was
/// refused.
fn drop_target(
    ui: &egui::Ui,
    s: &mut Session,
    row: &Row,
    response: &egui::Response,
    rect: Rect,
) -> Option<String> {
    let dragged = response.dnd_hover_payload::<Dragged>()?;
    let y = ui.input(|i| i.pointer.interact_pos())?.y;
    let at = layers::drop_at(row, (y - rect.top()) / rect.height());
    layers::drop_command(&s.doc, &dragged.0, at)?;
    let stroke = Stroke::new(2.0, theme::YELLOW);
    let painter = ui.painter();
    match at {
        Drop::Above(_) => {
            painter.hline(rect.x_range(), rect.top(), stroke);
        }
        Drop::Below(_) => {
            painter.hline(rect.x_range(), rect.bottom(), stroke);
        }
        Drop::Into(_) => {
            painter.rect_stroke(rect.shrink(1.0), 2.0, stroke, egui::StrokeKind::Inside);
        }
    }
    let dropped = response.dnd_release_payload::<Dragged>()?;
    layers::drop(s, &dropped.0, at).err()
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, CornerRadii, Geometry, Node, NodeKind, Rect, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn showing_the_panel_does_not_edit_the_document() {
        let mut doc = new_document("t", 100.0, 100.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let rect = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: CornerRadii::default(),
                },
                style: Style::default(),
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![rect],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        let before = s.doc.clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                assert_eq!(show(ui, &mut s), None);
            })
            .textures_delta
            .clear();
        }
        assert_eq!(s.doc, before);
        assert_eq!(s.edit_menu().undo, None);
        assert_eq!(s.selected(), vec![id]);
    }
}
