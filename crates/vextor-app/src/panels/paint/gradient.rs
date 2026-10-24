//! The gradient editor (spec 09): which gradient, its name, its spread;
//! then its stops on a strip as wide as the grid (click to add, drag to
//! move), the selected stop's position and its colour. Start, end,
//! centre, radius and focal are dragged on the canvas
//! (`canvas/gradient_handles.rs`). Laid out on the Transform grid.

use eframe::egui::{self, Color32, Mesh, Pos2, Rect, Sense, Stroke};
use vextor_core::{Command, Gradient, NodeId, Paint, Resource, ResourceId, Rgba, Spread};

use super::super::GRID_WIDTH;
use super::layout::{self, Row};
use crate::icons::{self, Icon};
use crate::paint_edit::{self, Edit, Target};
use crate::session::Session;
use crate::theme;

/// Height of the stop bar's colour strip and of a stop marker, in points.
const BAR_HEIGHT: f32 = 18.0;
const MARKER: egui::Vec2 = egui::vec2(8.0, 10.0);
/// A press this close to a marker (in x) grabs it, in points.
const GRAB: f32 = 6.0;

/// The editor for gradient `id`, painting `target` of `ids`. Returns the
/// command to run, if any.
pub(super) fn editor(
    ui: &mut egui::Ui,
    s: &mut Session,
    ids: &[NodeId],
    target: Target,
    id: ResourceId,
) -> Option<Command> {
    let Some(Resource::Gradient(g)) = s.doc.resources.get(&id).cloned() else {
        return None;
    };
    let mut cmd = None;
    if let Some(other) = super::resource_menu(ui, s, target, id, true) {
        let edit = Edit::Paint(Paint::Gradient(other));
        cmd = paint_edit::command(&s.doc, ids, target, &edit);
    }
    let named = super::name_row(ui, s, target, id);
    cmd = cmd.or(named);
    let row = Row::new(ui, "Spread");
    if let Some(spread) = row.put(ui, row.right_half(), |ui| spread_menu(ui, target, g.spread)) {
        cmd = cmd.or_else(|| Some(changed(id, &g, |g| g.spread = spread)));
    }
    layout::group_gap(ui);

    let key = egui::Id::unique(("gradient-stop", target, id));
    let mut selected = ui
        .data(|d| d.get_temp::<usize>(key))
        .unwrap_or(0)
        .min(g.stops.len().saturating_sub(1));
    let (edited, i) = stop_bar(ui, s, key, &g, selected);
    selected = i;
    let (fields, i) = stop_fields(ui, s, &g, selected);
    selected = i;
    if let Some(edited) = edited.or(fields) {
        cmd = cmd.or_else(|| Some(paint_edit::edit_resource(id, Resource::Gradient(edited))));
    }
    ui.data_mut(|d| d.insert_temp(key, selected));
    layout::group_gap(ui);
    if let Some(stop) = g.stops.get(selected) {
        let kind = target.swatch_kind();
        if let Some(c) = super::color::editor(ui, s, key.with("colour"), stop.color, kind) {
            cmd = cmd.or_else(|| Some(changed(id, &g, |g| g.stops[selected].color = c)));
        }
    }
    layout::group_gap(ui);
    ui.weak("Drag the gradient's handles on the canvas to place it.");
    cmd
}

/// The command editing gradient `id` (now `g`) by `f`.
fn changed(id: ResourceId, g: &Gradient, f: impl FnOnce(&mut Gradient)) -> Command {
    let mut out = g.clone();
    f(&mut out);
    paint_edit::edit_resource(id, Resource::Gradient(out))
}

fn spread_menu(ui: &mut egui::Ui, target: Target, current: Spread) -> Option<Spread> {
    let name = |s: Spread| match s {
        Spread::Pad => "Pad",
        Spread::Reflect => "Reflect",
        Spread::Repeat => "Repeat",
    };
    let mut picked = current;
    egui::ComboBox::from_id_salt(("gradient-spread", target))
        .selected_text(name(current))
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for s in [Spread::Pad, Spread::Reflect, Spread::Repeat] {
                ui.selectable_value(&mut picked, s, name(s));
            }
        })
        .response
        .on_hover_text("How the colours continue past the ends");
    (picked != current).then_some(picked)
}

fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The gradient as a strip with a marker under each stop. A click on the
/// strip adds a stop there; a marker is selected by a press and moved by
/// a drag (one undo step). Returns the edited gradient, if edited, and
/// the selected stop's index.
fn stop_bar(
    ui: &mut egui::Ui,
    s: &mut Session,
    key: egui::Id,
    g: &Gradient,
    selected: usize,
) -> (Option<Gradient>, usize) {
    let width = GRID_WIDTH;
    let (rect, r) = ui.allocate_exact_size(
        egui::vec2(width, BAR_HEIGHT + MARKER.y + 2.0),
        Sense::click_and_drag(),
    );
    let bar = Rect::from_min_size(rect.min, egui::vec2(width, BAR_HEIGHT));
    let inner = bar.shrink2(egui::vec2(MARKER.x / 2.0, 0.0));
    let x_of = |offset: f64| stop_x(inner, offset);
    let offset_at = |x: f32| f64::from(((x - inner.left()) / inner.width()).clamp(0.0, 1.0));
    paint_strip(ui, bar, inner, g);
    paint_markers(ui, bar, inner, g, selected);
    let r = r.on_hover_text("Click to add a stop; drag a marker to move it");

    let dragging = key.with("dragging");
    let pointer = r.interact_pointer_pos();
    let near = |p: Pos2| {
        g.stops
            .iter()
            .enumerate()
            .map(|(i, st)| (i, (x_of(st.offset) - p.x).abs()))
            .filter(|(_, d)| *d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if ui.input(|i| i.pointer.primary_pressed())
        && let Some(p) = pointer
        && let Some(i) = near(p)
    {
        ui.data_mut(|d| d.insert_temp(dragging, i));
        return (None, i);
    }
    if r.drag_started() {
        s.begin_gesture();
    }
    let grabbed = ui.data(|d| d.get_temp::<usize>(dragging));
    let mut out = None;
    if let (Some(i), Some(p)) = (grabbed, pointer)
        && r.dragged()
    {
        out = paint_edit::move_stop(g, i, offset_at(p.x));
        if let Some((_, at)) = out {
            ui.data_mut(|d| d.insert_temp(dragging, at));
        }
    } else if r.clicked()
        && grabbed.is_none()
        && let Some(p) = pointer
    {
        out = Some(paint_edit::add_stop(g, offset_at(p.x)));
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
    if !ui.input(|i| i.pointer.any_down()) {
        ui.data_mut(|d| d.remove::<usize>(dragging));
    }
    match out {
        Some((edited, i)) => (Some(edited), i),
        None => (None, selected),
    }
}

/// Where a stop at `offset` sits along the strip.
fn stop_x(inner: Rect, offset: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)] // offsets are 0..=1
    let t = offset as f32;
    inner.left() + inner.width() * t
}

/// A marker under each stop in its colour, the selected one outlined in
/// yellow.
fn paint_markers(ui: &egui::Ui, bar: Rect, inner: Rect, g: &Gradient, selected: usize) {
    for (i, stop) in g.stops.iter().enumerate() {
        let m = Rect::from_min_size(
            Pos2::new(
                stop_x(inner, stop.offset) - MARKER.x / 2.0,
                bar.bottom() + 2.0,
            ),
            MARKER,
        );
        ui.painter().rect_filled(m, 1.0, color32(stop.color));
        let (width, edge) = if i == selected {
            (2.0, theme::YELLOW)
        } else {
            (1.0, theme::GREY_300)
        };
        ui.painter()
            .rect_stroke(m, 1.0, Stroke::new(width, edge), egui::StrokeKind::Outside);
    }
}

/// The strip: a checkerboard (so alpha shows) under the colours, each
/// stop's colour flat before the first and after the last stop.
fn paint_strip(ui: &egui::Ui, bar: Rect, inner: Rect, g: &Gradient) {
    let painter = ui.painter();
    painter.rect_filled(bar, 2.0, theme::GREY_100);
    let cell = BAR_HEIGHT / 2.0;
    let mut x = bar.left();
    let mut column = 0;
    while x < bar.right() {
        let w = cell.min(bar.right() - x);
        let y = if column % 2 == 0 {
            bar.top()
        } else {
            bar.top() + cell
        };
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, y), egui::vec2(w, cell)),
            0.0,
            theme::GREY_400,
        );
        x += cell;
        column += 1;
    }
    let mut mesh = Mesh::default();
    let mut band = |x0: f32, c0: Color32, x1: f32, c1: Color32| {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX - 4);
        mesh.colored_vertex(Pos2::new(x0, bar.top()), c0);
        mesh.colored_vertex(Pos2::new(x1, bar.top()), c1);
        mesh.colored_vertex(Pos2::new(x1, bar.bottom()), c1);
        mesh.colored_vertex(Pos2::new(x0, bar.bottom()), c0);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    };
    let x_of = |o: f64| stop_x(inner, o);
    if let (Some(first), Some(last)) = (g.stops.first(), g.stops.last()) {
        band(
            bar.left(),
            color32(first.color),
            x_of(first.offset),
            color32(first.color),
        );
        for w in g.stops.windows(2) {
            band(
                x_of(w[0].offset),
                color32(w[0].color),
                x_of(w[1].offset),
                color32(w[1].color),
            );
        }
        band(
            x_of(last.offset),
            color32(last.color),
            bar.right(),
            color32(last.color),
        );
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        bar,
        2.0,
        Stroke::new(1.0, theme::GREY_500),
        egui::StrokeKind::Inside,
    );
}

/// The selected stop's position (0–100 %) and its Remove button. Returns
/// the edited gradient, if edited, and the stop's index after the edit.
fn stop_fields(
    ui: &mut egui::Ui,
    s: &mut Session,
    g: &Gradient,
    selected: usize,
) -> (Option<Gradient>, usize) {
    let Some(stop) = g.stops.get(selected) else {
        return (None, selected);
    };
    let mut out = None;
    // Like the Name row: the value in the right half, the bin closing it.
    let row = Row::new(ui, "Stop");
    let (field, bin) = row.right_half_and_button();
    let mut percent = stop.offset * 100.0;
    let r = row.put(ui, field, |ui| {
        ui.add_sized(
            ui.available_size(),
            egui::DragValue::new(&mut percent)
                .range(0.0..=100.0)
                .suffix(" %")
                .max_decimals(1),
        )
    });
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.changed() {
        out = paint_edit::move_stop(g, selected, percent / 100.0);
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
    let can = g.stops.len() > 2;
    let why = "A gradient keeps at least two stops";
    let removed = row.put(ui, bin, |ui| {
        icons::quiet_button(ui, Icon::Delete, bin.size(), false, can, "Remove stop", why).clicked()
    });
    if removed && let Some(edited) = paint_edit::remove_stop(g, selected) {
        out = Some((edited, selected.saturating_sub(1)));
    }
    match out {
        Some((edited, i)) => (Some(edited), i),
        None => (None, selected),
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::Document;

    use super::*;

    /// Runs one frame of the stop bar with `events`; returns where the bar
    /// started and what it returned.
    fn frame(
        ctx: &egui::Context,
        s: &mut Session,
        g: &Gradient,
        selected: usize,
        events: Vec<egui::Event>,
    ) -> (Pos2, (Option<Gradient>, usize)) {
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut out = (Pos2::ZERO, (None, selected));
        ctx.run_ui(input, |ui| {
            let at = ui.cursor().min;
            out = (
                at,
                stop_bar(ui, s, egui::Id::unique("test-stops"), g, selected),
            );
        })
        .textures_delta
        .clear();
        out
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn clicking_a_stop_selects_it_without_an_undo_step() {
        let mut s = Session::new(Document::new("stops"));
        let g = paint_edit::new_gradient(paint_edit::GradientShape::Linear, Rgba::BLACK, None);
        let ctx = egui::Context::default();
        let (origin, _) = frame(&ctx, &mut s, &g, 0, vec![]);
        // The last stop's marker: right end of the strip, under it.
        let width = GRID_WIDTH;
        let marker = origin + egui::vec2(width - MARKER.x / 2.0, BAR_HEIGHT + 6.0);
        let mut selected = 0;
        for events in [
            vec![egui::Event::PointerMoved(marker)],
            vec![button(marker, true)],
            vec![button(marker, false)],
            vec![],
        ] {
            let (_, (edited, i)) = frame(&ctx, &mut s, &g, selected, events);
            assert_eq!(edited, None, "selecting is not an edit");
            selected = i;
        }
        assert_eq!(selected, 1, "the last stop is selected");
        assert_eq!(s.edit_menu().undo, None, "nothing was recorded");
    }
}
