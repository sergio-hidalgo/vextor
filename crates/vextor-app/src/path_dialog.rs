//! The path operations' dialogs (spec 11): Offset Path… and Simplify…,
//! whose settings show live on the canvas, and "Convert text to outlines
//! first?" before a boolean on text.
//!
//! The preview is the real result: each change takes the previous preview
//! back (undo) and applies the new one, so the canvas shows true fills and
//! strokes. OK keeps it as one undo step; Cancel, `Esc` or a click outside
//! takes it back.

use eframe::egui::{self, Rect, Vec2};
use vextor_core::LineJoin;

use crate::icons::{self, Icon};
use crate::prompts::{
    SHAPE_DIALOG_WIDTH, SHAPE_FIELD_SIZE, body, buttons, dialog, dialog_of_width, field_row,
};
use crate::session::Session;
use crate::shape_ops::{self, OffsetSettings, ShapeOp};

/// Offsets the dialog accepts, in px either way.
const MAX_OFFSET: f64 = 1_000.0;
/// Simplify's tolerance range, in px.
const TOLERANCE: std::ops::RangeInclusive<f64> = 0.01..=10.0;

/// The join buttons: join, icon, tooltip.
const JOINS: [(LineJoin, Icon, &str); 3] = [
    (LineJoin::Miter, Icon::JoinMiter, "Miter"),
    (LineJoin::Round, Icon::JoinRound, "Round"),
    (LineJoin::Bevel, Icon::JoinBevel, "Bevel"),
];

/// What a dialog is set to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Settings {
    Offset(OffsetSettings),
    Simplify(f64),
}

/// An open Offset Path… or Simplify… dialog.
#[derive(Debug)]
pub struct PathDialog {
    settings: Settings,
    /// The settings the canvas shows, once tried.
    shown: Option<Settings>,
    /// The document holds a preview step to take back.
    applied: bool,
    /// Why the settings shown cannot be applied.
    error: Option<String>,
    /// Simplify: nodes before and after.
    counts: Option<(usize, usize)>,
}

impl PathDialog {
    /// The dialog of `op`, when it has one.
    #[must_use]
    pub fn new(op: ShapeOp) -> Option<Self> {
        let settings = match op {
            ShapeOp::Offset => Settings::Offset(OffsetSettings::default()),
            ShapeOp::Simplify => Settings::Simplify(shape_ops::DEFAULT_TOLERANCE),
            _ => return None,
        };
        Some(Self {
            settings,
            shown: None,
            applied: false,
            error: None,
            counts: None,
        })
    }

    /// Shows the dialog centred on `canvas`, previewing on `s`. Returns
    /// true once it is closed (kept or taken back).
    pub fn show(&mut self, ctx: &egui::Context, canvas: Rect, s: &mut Session) -> bool {
        let title = match self.settings {
            Settings::Offset(_) => "Offset Path",
            Settings::Simplify(_) => "Simplify",
        };
        let modal = dialog_of_width(ctx, "path_dialog", canvas, SHAPE_DIALOG_WIDTH, |ui| {
            ui.add_space(4.0);
            ui.heading(title);
            ui.add_space(10.0);
            match &mut self.settings {
                Settings::Offset(o) => offset_fields(ui, o),
                Settings::Simplify(t) => {
                    field_row(ui, "Tolerance", |ui| {
                        ui.add_sized(
                            SHAPE_FIELD_SIZE,
                            egui::DragValue::new(t)
                                .range(TOLERANCE)
                                .speed(0.01)
                                .suffix(" px")
                                .max_decimals(2),
                        )
                        .on_hover_text("How far the simpler path may stray from the original");
                    });
                    if let Some((before, after)) = self.counts {
                        ui.add_space(4.0);
                        ui.weak(format!("{before} → {after} nodes"));
                    }
                }
            }
            if let Some(e) = &self.error {
                ui.add_space(4.0);
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(12.0);
            buttons(ui, &[("OK", true), ("Cancel", false)])
        });
        self.preview(s);
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let answer = match modal.inner {
            Some(keep) => Some(keep),
            None if enter => Some(true),
            None if modal.should_close() => Some(false),
            None => None,
        };
        match answer {
            // OK keeps the preview; with settings that failed there is
            // nothing to keep, so the dialog stays.
            Some(true) => self.error.is_none(),
            Some(false) => {
                self.take_back(s);
                true
            }
            None => false,
        }
    }

    /// Puts the current settings on the canvas when they changed.
    fn preview(&mut self, s: &mut Session) {
        if self.shown == Some(self.settings) {
            return;
        }
        self.take_back(s);
        let revision = s.revision();
        let result = match self.settings {
            Settings::Offset(o) => shape_ops::offset_selection(s, o),
            Settings::Simplify(t) => shape_ops::simplify_selection(s, t).map(|counts| {
                self.counts = Some(counts);
            }),
        };
        self.applied = s.revision() != revision;
        self.error = result.err();
        self.shown = Some(self.settings);
    }

    /// Undoes the preview step, if one is in the document.
    fn take_back(&mut self, s: &mut Session) {
        if self.applied {
            s.undo();
            self.applied = false;
        }
    }
}

/// Distance, joins (three icon buttons) and miter limit.
fn offset_fields(ui: &mut egui::Ui, o: &mut OffsetSettings) {
    field_row(ui, "Distance", |ui| {
        ui.add_sized(
            SHAPE_FIELD_SIZE,
            egui::DragValue::new(&mut o.distance)
                .range(-MAX_OFFSET..=MAX_OFFSET)
                .speed(0.1)
                .suffix(" px")
                .max_decimals(2),
        )
        .on_hover_text("Outward; negative insets");
    });
    field_row(ui, "Joins", |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let size = Vec2::new((SHAPE_FIELD_SIZE.x - 4.0) / 3.0, SHAPE_FIELD_SIZE.y);
        for (join, icon, tooltip) in JOINS {
            if icons::choice_button(ui, icon, size, o.join == join, tooltip).clicked() {
                o.join = join;
            }
        }
    });
    field_row(ui, "Miter limit", |ui| {
        ui.add_enabled(
            o.join == LineJoin::Miter,
            egui::DragValue::new(&mut o.miter_limit)
                .range(1.0..=100.0)
                .speed(0.1)
                .max_decimals(1),
        )
        .on_hover_text("Longer miters than this × the distance are bevelled");
    });
}

/// Asks before a boolean on text: the text becomes outlines first, in the
/// same undo step. `missing`: a font that is not installed and the one
/// shown instead. `Some(true)` goes on, `Some(false)` (or `Esc`, or a
/// click outside) cancels, `None` while the user has not answered.
pub fn outlines_first(
    ctx: &egui::Context,
    canvas: Rect,
    op: ShapeOp,
    missing: Option<&vextor_geom::MissingFont>,
) -> Option<bool> {
    let note = missing.map_or_else(String::new, |m| {
        let shown = m.shown.as_deref().unwrap_or("another font");
        format!(
            " “{}” is not installed: the letters take the shape of {shown}.",
            m.wanted
        )
    });
    let text = format!(
        "{} works on shapes, so the selected text becomes outlines (letter shapes) first. It is no longer editable as text.{note}",
        op.title()
    );
    let modal = dialog(ctx, "outlines_first", canvas, |ui| {
        body(ui, "Convert text to outlines first?", &text);
        buttons(ui, &[("Convert", true), ("Cancel", false)])
    });
    if modal.should_close() && modal.inner.is_none() {
        return Some(false);
    }
    modal.inner
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, CornerRadii, Geometry, Node, NodeKind, Rect, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    fn one_rect() -> (Session, vextor_core::NodeId) {
        let mut doc = new_document("a", 200.0, 200.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let shape = Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(0.0, 0.0, 50.0, 50.0),
                radius: CornerRadii::default(),
            },
            style: Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(shape))],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        (s, id)
    }

    fn geometry(s: &Session, id: vextor_core::NodeId) -> Geometry {
        match &s.doc.nodes[&id].kind {
            NodeKind::Shape(sh) => sh.geometry.clone(),
            _ => panic!("a shape"),
        }
    }

    #[test]
    fn each_change_replaces_the_preview_so_ok_is_one_undo_step() {
        let (mut s, id) = one_rect();
        let mut d = PathDialog::new(ShapeOp::Offset).expect("a dialog");
        d.preview(&mut s);
        let first = geometry(&s, id);
        assert!(
            matches!(first, Geometry::Path(_)),
            "the preview is in the document"
        );
        for distance in [6.0, 8.0, 10.0] {
            d.settings = Settings::Offset(OffsetSettings {
                distance,
                ..OffsetSettings::default()
            });
            d.preview(&mut s);
        }
        assert_ne!(geometry(&s, id), first);
        assert_eq!(s.edit_menu().undo, Some(shape_ops::OFFSET));
        s.undo();
        assert!(
            matches!(geometry(&s, id), Geometry::Rect { .. }),
            "one undo takes the whole dialog back"
        );
    }

    #[test]
    fn cancel_takes_the_preview_back_and_a_failed_setting_leaves_nothing() {
        let (mut s, id) = one_rect();
        let mut d = PathDialog::new(ShapeOp::Offset).expect("a dialog");
        d.preview(&mut s);
        d.settings = Settings::Offset(OffsetSettings {
            distance: -40.0,
            ..OffsetSettings::default()
        });
        d.preview(&mut s);
        assert!(d.error.is_some(), "an inset past the middle");
        assert!(matches!(geometry(&s, id), Geometry::Rect { .. }));
        d.take_back(&mut s);
        assert!(matches!(geometry(&s, id), Geometry::Rect { .. }));
        assert_eq!(s.edit_menu().undo, None, "nothing left behind");
    }
}
