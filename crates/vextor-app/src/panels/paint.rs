//! Inspector ▸ Fill, Stroke and Opacity (spec 09), and the paint picker
//! the Text section reuses for a text's background and border. Values show
//! the first selected object; a change applies to every selected object
//! that has that paint. The rules live in `crate::paint_edit`.

mod color;
mod gradient;
mod layout;
mod pattern;
mod stroke;

use eframe::egui;
use vextor_core::{Command, GradientKind, NodeId, Paint, Resource, ResourceId, Rgba};

pub use color::escape_bad_hex;

use self::layout::Row;
use super::GRID_SPACING;
use crate::icons::{self, Icon};
use crate::paint_edit::{self, Edit, GradientShape, Target};
use crate::session::Session;

/// What a paint is, as the kind menu lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    None,
    Solid,
    Linear,
    Radial,
    Pattern,
    CurrentColor,
}

impl Kind {
    const ALL: [Self; 6] = [
        Self::None,
        Self::Solid,
        Self::Linear,
        Self::Radial,
        Self::Pattern,
        Self::CurrentColor,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Solid => "Solid colour",
            Self::Linear => "Linear gradient",
            Self::Radial => "Radial gradient",
            Self::Pattern => "Pattern",
            Self::CurrentColor => "currentColor",
        }
    }

    fn of(s: &Session, paint: Paint) -> Self {
        match paint {
            Paint::None => Self::None,
            Paint::Solid(_) => Self::Solid,
            Paint::CurrentColor => Self::CurrentColor,
            Paint::Pattern(_) => Self::Pattern,
            Paint::Gradient(id) => match s.doc.resources.get(&id) {
                Some(Resource::Gradient(g)) if matches!(g.kind, GradientKind::Radial { .. }) => {
                    Self::Radial
                }
                _ => Self::Linear,
            },
        }
    }
}

/// Fill, Stroke and Opacity for the selected objects. Returns a message
/// when an edit was refused.
pub fn sections(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let ids = s.selected();
    let mut message = None;
    for (target, title) in [(Target::Fill, "Fill"), (Target::Stroke, "Stroke")] {
        let on = paint_edit::targets(&s.doc, &ids, target);
        if !on.is_empty() {
            super::section(ui, title, |ui| {
                message = picker(ui, s, &on, target).or(message.take());
            });
        }
    }
    if !ids.is_empty() {
        super::section(ui, "Opacity", |ui| {
            message = opacity(ui, s, &ids).or(message.take());
        });
    }
    message
}

/// The paint picker for `target` of `ids` (objects it applies to, not
/// empty): the kind menu, then that kind's fields.
pub(super) fn picker(
    ui: &mut egui::Ui,
    s: &mut Session,
    ids: &[NodeId],
    target: Target,
) -> Option<String> {
    let first = s.doc.nodes.get(ids.first()?)?;
    let current = paint_edit::paint(first, target)?;
    let current_stroke = paint_edit::stroke(first, target);
    let mut cmd: Option<Command> = None;
    // Rows as far apart as Transform's; groups further.
    ui.spacing_mut().item_spacing.y = GRID_SPACING[1];
    // From under the colour tile to the end of the number fields.
    let row = Row::new(ui, "Paint");
    let kind = row.put(ui, row.wide(), |ui| {
        kind_menu(ui, s, target, Kind::of(s, current))
    });
    if let Some(kind) = kind {
        cmd = switch_to(s, ids, target, current, kind);
    }
    if current != Paint::None {
        layout::group_gap(ui);
    }
    let key = egui::Id::unique(("paint-colour", target));
    match current {
        Paint::Solid(c) => {
            if let Some(c) = color::editor(ui, s, key, c, target.swatch_kind()) {
                let edit = Edit::Paint(Paint::Solid(c));
                cmd = cmd.or_else(|| paint_edit::command(&s.doc, ids, target, &edit));
            }
        }
        Paint::Gradient(id) => cmd = cmd.or(gradient::editor(ui, s, ids, target, id)),
        Paint::Pattern(id) => cmd = cmd.or(pattern::editor(ui, s, ids, target, id)),
        Paint::CurrentColor => {
            ui.weak("Takes the CSS color of the page, so an icon follows the text around it.");
        }
        Paint::None => {}
    }
    if let Some(st) = &current_stroke
        && let Some(edit) = {
            layout::group_gap(ui);
            stroke::fields(ui, s, target, st)
        }
    {
        cmd = cmd.or_else(|| paint_edit::command(&s.doc, ids, target, &edit));
    }
    let failed = cmd.and_then(|c| s.apply(c).err());
    failed.map(|e| format!("{} failed: {e}", target.label()))
}

/// The kind menu; the kind picked, if it changed. Pattern is disabled
/// while the document has none.
fn kind_menu(ui: &mut egui::Ui, s: &Session, target: Target, current: Kind) -> Option<Kind> {
    let has_pattern = s
        .doc
        .resources
        .values()
        .any(|r| matches!(r, Resource::Pattern(_)));
    let mut picked = current;
    egui::ComboBox::from_id_salt(("paint-kind-menu", target))
        .selected_text(current.name())
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for kind in Kind::ALL {
                let enabled = kind != Kind::Pattern || has_pattern;
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(&mut picked, kind, kind.name());
                })
                .response
                .on_disabled_hover_text(
                    "Make a pattern first: select objects, then Object ▸ Make Pattern",
                );
            }
        });
    (picked != current).then_some(picked)
}

/// The command that turns `target` of `ids` from `current` into `kind`:
/// a solid takes the colour shown so far; a gradient is new, copying the
/// stops of the current one; a pattern is the first one in the document.
fn switch_to(
    s: &mut Session,
    ids: &[NodeId],
    target: Target,
    current: Paint,
    kind: Kind,
) -> Option<Command> {
    let base = base_color(s, current);
    let paint = match kind {
        Kind::None => Paint::None,
        Kind::Solid => Paint::Solid(base),
        Kind::CurrentColor => Paint::CurrentColor,
        Kind::Pattern => Paint::Pattern(first_pattern(s)?),
        Kind::Linear | Kind::Radial => {
            let shape = if kind == Kind::Linear {
                GradientShape::Linear
            } else {
                GradientShape::Radial
            };
            let like = match current {
                Paint::Gradient(id) => match s.doc.resources.get(&id) {
                    Some(Resource::Gradient(g)) => Some(g.clone()),
                    _ => None,
                },
                _ => None,
            };
            let g = paint_edit::new_gradient(shape, base, like.as_ref());
            return Some(paint_edit::command_with_resource(
                &mut s.doc,
                ids,
                target,
                Resource::Gradient(g),
                Paint::Gradient,
            ));
        }
    };
    paint_edit::command(&s.doc, ids, target, &Edit::Paint(paint))
}

/// The colour a paint shows most: a solid's, a gradient's first stop,
/// else black.
fn base_color(s: &Session, paint: Paint) -> Rgba {
    match paint {
        Paint::Solid(c) => c,
        Paint::Gradient(id) => match s.doc.resources.get(&id) {
            Some(Resource::Gradient(g)) => g.stops.first().map_or(Rgba::BLACK, |st| st.color),
            _ => Rgba::BLACK,
        },
        _ => Rgba::BLACK,
    }
}

fn first_pattern(s: &Session) -> Option<ResourceId> {
    s.doc
        .resources
        .iter()
        .find(|(_, r)| matches!(r, Resource::Pattern(_)))
        .map(|(id, _)| *id)
}

/// The resources of one kind for a "which one" menu: id and name
/// ("Gradient 2" when unnamed), in id order.
fn resource_names(s: &Session, gradients: bool) -> Vec<(ResourceId, String)> {
    let mut n = 0;
    s.doc
        .resources
        .iter()
        .filter(|(_, r)| matches!(r, Resource::Gradient(_)) == gradients)
        .map(|(id, r)| {
            n += 1;
            let name = match r {
                Resource::Gradient(g) => g.name.clone(),
                Resource::Pattern(p) => p.name.clone(),
            };
            let fallback = if gradients { "Gradient" } else { "Pattern" };
            (*id, name.unwrap_or_else(|| format!("{fallback} {n}")))
        })
        .collect()
}

/// The Gradient (or Pattern) row: a menu of the document's gradients (or
/// patterns) showing `current`, in the right half; the one picked, if it
/// changed. With more than one user it says how many share it.
fn resource_menu(
    ui: &mut egui::Ui,
    s: &Session,
    target: Target,
    current: ResourceId,
    gradients: bool,
) -> Option<ResourceId> {
    let names = resource_names(s, gradients);
    let users = paint_edit::users(&s.doc, current);
    let shown = names
        .iter()
        .find(|(id, _)| *id == current)
        .map_or_else(String::new, |(_, n)| n.clone());
    let shown = if users > 1 {
        format!("{shown} · {users} uses")
    } else {
        shown
    };
    let mut picked = current;
    let row = Row::new(ui, if gradients { "Gradient" } else { "Pattern" });
    row.put(ui, row.right_half(), |ui| {
        egui::ComboBox::from_id_salt(("paint-resource", target))
            .selected_text(shown)
            .width(ui.available_width())
            .truncate()
            .show_ui(ui, |ui| {
                for (id, name) in &names {
                    ui.selectable_value(&mut picked, *id, name);
                }
            })
            .response
            .on_hover_text(format!(
                "Used by {users} object(s); editing it changes every one"
            ));
    });
    (picked != current).then_some(picked)
}

/// The Name row of gradient or pattern `id`: its name in the right half
/// (applied when the field is left; `Esc` puts it back) and, closing the
/// row, a Delete button that takes it off every object painted with it.
/// One undo step each.
fn name_row(ui: &mut egui::Ui, s: &Session, target: Target, id: ResourceId) -> Option<Command> {
    let resource = s.doc.resources.get(&id)?;
    let (name, word) = match resource {
        Resource::Gradient(g) => (g.name.clone(), "gradient"),
        Resource::Pattern(p) => (p.name.clone(), "pattern"),
    };
    let key = egui::Id::unique(("paint-resource-name", target, id));
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| name.unwrap_or_default());
    let mut cmd = None;
    let row = Row::new(ui, "Name");
    let (field, bin) = row.right_half_and_button();
    row.put(ui, field, |ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut text)
                .char_limit(paint_edit::MAX_RESOURCE_NAME_CHARS)
                .hint_text(format!("Name this {word}"))
                .desired_width(f32::INFINITY),
        );
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        }
        if r.lost_focus() {
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                cmd = paint_edit::rename_resource(&s.doc, id, &text);
            }
            ui.data_mut(|d| d.remove::<String>(key));
        }
    });
    let tip = format!("Delete {word}");
    let deleted = row.put(ui, bin, |ui| {
        icons::quiet_button(ui, Icon::Delete, bin.size(), false, true, &tip, "").clicked()
    });
    if deleted {
        cmd = paint_edit::delete_resource(&s.doc, id);
    }
    cmd
}

/// Object opacity: a slider from the left-hand field to the second
/// label's end, its percentage in the right-hand field column (like the
/// colour's A). A drag is one undo step.
fn opacity(ui: &mut egui::Ui, s: &mut Session, ids: &[NodeId]) -> Option<String> {
    let current = s.doc.nodes.get(ids.first()?)?.opacity;
    let mut percent = current * 100.0;
    let row = Row::new(ui, "");
    let slid = row.put(ui, row.slider(), |ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).show_value(false))
    });
    let typed = row.put(ui, row.field2(), |ui| {
        ui.add_sized(
            ui.available_size(),
            egui::DragValue::new(&mut percent)
                .range(0.0..=100.0)
                .suffix(" %")
                .max_decimals(0),
        )
    });
    let mut failed = None;
    for r in [&slid, &typed] {
        if r.drag_started() {
            s.begin_gesture();
        }
    }
    if slid.changed() || typed.changed() {
        failed = paint_edit::opacity_command(&s.doc, ids, percent / 100.0)
            .and_then(|c| s.apply(c).err());
    }
    for r in [&slid, &typed] {
        if r.drag_stopped() {
            s.end_gesture();
        }
    }
    failed.map(|e| format!("Change opacity failed: {e}"))
}

#[cfg(test)]
mod tests {
    use vextor_core::{
        Command, Geometry, Node, NodeKind, Rect, Shape, Stroke, Style, Text, TextFrame,
    };

    use super::*;
    use crate::new_doc::new_document;

    /// Shows the inspector's paint sections twice (egui settles layout on
    /// the second frame) for the current selection.
    fn show(s: &mut Session) -> Option<String> {
        let ctx = egui::Context::default();
        let mut message = None;
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                message = sections(ui, s);
                // The Text section shows the frame's paint pickers.
                message = super::super::text::section(ui, s).or(message.take());
            })
            .textures_delta
            .clear();
        }
        message
    }

    #[test]
    fn showing_every_kind_of_paint_changes_nothing() {
        let mut doc = new_document("p", 100.0, 100.0);
        let g = paint_edit::new_gradient(GradientShape::Radial, Rgba::rgb(255, 0, 0), None);
        let gid = doc.alloc_resource_id();
        doc.apply(Command::PutResource {
            id: gid,
            resource: Resource::Gradient(g),
            nodes: vec![],
        })
        .expect("gradient");
        let rect = doc.alloc_node_id();
        let text = doc.alloc_node_id();
        let shape = Node::new(
            rect,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: vextor_core::CornerRadii::default(),
                },
                style: Style {
                    fill: Paint::Gradient(gid),
                    stroke: Some(Stroke {
                        dash: vec![5.0, 1.0],
                        ..Stroke::default()
                    }),
                },
            }),
        );
        let label = Node::new(
            text,
            NodeKind::Text(Text {
                frame: Some(TextFrame {
                    fill: Paint::CurrentColor,
                    stroke: Some(Stroke::default()),
                    ..paint_edit::empty_frame()
                }),
                ..Text::new("Buy", "Inter")
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(doc.layers[0]),
            index: 0,
            nodes: vec![shape, label],
        })
        .expect("nodes");
        // A pattern of the rect paints its stroke.
        let (cmd, pid) = doc
            .make_pattern(&[rect], Rect::new(0.0, 0.0, 10.0, 10.0))
            .expect("pattern");
        doc.apply(cmd).expect("pattern made");
        let NodeKind::Shape(sh) = &doc.nodes[&rect].kind else {
            unreachable!("shape")
        };
        let mut style = sh.style.clone();
        if let Some(st) = &mut style.stroke {
            st.paint = Paint::Pattern(pid);
        }
        doc.apply(Command::SetStyle {
            ids: vec![rect],
            style,
        })
        .expect("pattern stroke");
        let mut s = Session::new(doc);
        let before = s.doc.clone();
        for selection in [vec![rect], vec![text], vec![rect, text]] {
            s.select(selection);
            assert_eq!(show(&mut s), None);
        }
        assert_eq!(s.doc, before);
        assert_eq!(s.edit_menu().undo, None, "nothing was recorded");
    }
}
