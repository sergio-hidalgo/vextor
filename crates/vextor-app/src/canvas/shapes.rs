//! The canvas side of the shape tools and rect corner widgets (spec 06):
//! pointer and keys to `shape_tool` and `corners`, the size dialog after
//! a click, and the widgets' dots.

use eframe::egui::{self, Key, Painter, Pos2, Stroke};
use vextor_core::{NodeId, ToolId};
use vextor_geom::{DrawMods, ShapeSize};

use super::{Canvas, to_egui};
use crate::corners::{self, CornerDrag};
use crate::select::Mods;
use crate::session::Session;
use crate::shape_tool::{self, DrawKey, Release, ShapeTool};
use crate::{prompts, theme, view};

/// Radius of a drawn corner widget, in points.
const WIDGET_RADIUS: f32 = 4.0;

/// The canvas's shape state between frames.
#[derive(Debug, Default)]
pub(super) struct ShapeInput {
    tool: ShapeTool,
    /// Alt was held when the shape tool's press began.
    alt_at_press: bool,
    /// A rect corner widget being dragged.
    corner: Option<CornerDrag>,
    /// The size dialog after a click with a shape tool.
    size_dialog: Option<SizeDialog>,
}

impl ShapeInput {
    /// While a shape is drawn or a corner dragged: the press-time document,
    /// the objects lifted out of it and those rendered live each frame.
    pub(super) fn preview(&self) -> Option<(&vextor_core::Document, Vec<NodeId>, Vec<NodeId>)> {
        if let Some((before, id)) = self.tool.preview() {
            return Some((before, Vec::new(), id.into_iter().collect()));
        }
        let (before, id) = self.corner.as_ref()?.preview();
        Some((before, vec![id], vec![id]))
    }

    /// The state for another document: only the polygon and star options
    /// carry over (a drag or dialog belonged to the old document).
    pub(super) fn carry_over(&self) -> Self {
        let mut tool = ShapeTool::default();
        (tool.sides, tool.points, tool.inner) =
            (self.tool.sides, self.tool.points, self.tool.inner);
        Self {
            tool,
            ..Self::default()
        }
    }
}

/// The size dialog after a click with a shape tool.
#[derive(Debug)]
struct SizeDialog {
    tool: ToolId,
    at: kurbo::Point,
    /// Alt was held: the click is the centre.
    centred: bool,
    size: ShapeSize,
}

impl Canvas {
    /// True while a shape is drawn, a corner dragged or the size dialog
    /// is open: the app's shortcuts wait.
    #[must_use]
    pub fn is_drawing(&self) -> bool {
        self.shapes.tool.is_drawing()
            || self.shapes.corner.is_some()
            || self.shapes.size_dialog.is_some()
    }

    /// A press at `p` (document space) for a shape tool or a corner
    /// widget. Returns true when the press was used, and a message when
    /// drawing is refused.
    pub(super) fn shape_press(
        &mut self,
        session: &mut Session,
        p: kurbo::Point,
        mods: Mods,
    ) -> (bool, Option<String>) {
        let tool = session.editor.tool;
        if tool.is_shape() {
            self.shapes.alt_at_press = mods.alt;
            return (true, self.shapes.tool.press(session, tool, p).err());
        }
        if tool == ToolId::Select {
            self.shapes.corner = CornerDrag::press(session, p, session.editor.view.zoom);
            return (self.shapes.corner.is_some(), None);
        }
        (false, None)
    }

    /// The pointer at `at` with the button `down`, or released.
    pub(super) fn shape_drag(
        &mut self,
        session: &mut Session,
        at: Option<kurbo::Point>,
        down: bool,
        mods: Mods,
    ) -> Option<String> {
        let mut message = None;
        if let Some(drag) = &self.shapes.corner {
            if down && let Some(p) = at {
                message = drag.drag_to(session, p, mods.alt).err();
            }
            if !down && let Some(drag) = self.shapes.corner.take() {
                drag.release(session);
            }
        }
        if self.shapes.tool.is_drawing() {
            if down && let Some(p) = at {
                let mods = DrawMods {
                    constrain: mods.shift,
                    from_centre: mods.alt,
                };
                message = self.shapes.tool.drag_to(session, p, mods).err();
            }
            if !down && let Release::Clicked(p) = self.shapes.tool.release(session) {
                let tool = session.editor.tool;
                self.shapes.size_dialog = Some(SizeDialog {
                    tool,
                    at: p,
                    centred: self.shapes.alt_at_press,
                    size: self.shapes.tool.default_size(tool),
                });
            }
        }
        message
    }

    /// While drawing: arrows change sides, points and the star's inner
    /// radius; `Esc` takes the shape back.
    pub(super) fn shape_keys(&mut self, ui: &egui::Ui, session: &mut Session) -> Option<String> {
        if !self.shapes.tool.is_drawing() {
            return None;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.shapes.tool.cancel(session);
            return None;
        }
        let keys = [
            (Key::ArrowUp, DrawKey::More),
            (Key::ArrowDown, DrawKey::Fewer),
            (Key::ArrowRight, DrawKey::Fatter),
            (Key::ArrowLeft, DrawKey::Thinner),
        ];
        let mut message = None;
        for (key, k) in keys {
            // Shift and Alt shape the drag; the arrows work with either.
            let pressed = ui.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(
                    |e| !matches!(e, egui::Event::Key { key: k, pressed: true, .. } if *k == key),
                );
                i.events.len() != before
            });
            if pressed && let Err(e) = self.shapes.tool.key(session, k) {
                message = Some(e);
            }
        }
        message
    }

    /// The size dialog, while open; places the shape when answered.
    pub(super) fn size_dialog_ui(
        &mut self,
        ctx: &egui::Context,
        canvas: egui::Rect,
        session: &mut Session,
    ) -> Option<String> {
        let d = self.shapes.size_dialog.as_mut()?;
        let title = shape_tool::label(d.tool).trim_start_matches("Draw ");
        let answer = prompts::shape_size(ctx, canvas, title, &mut d.size)?;
        let d = self.shapes.size_dialog.take()?;
        if !answer {
            return None;
        }
        shape_tool::place(session, d.tool, d.size, d.at, d.centred).err()
    }

    /// True when the pointer at `p` (document space) is over a corner
    /// widget or one is dragged.
    pub(super) fn over_corner_widget(&self, session: &Session, p: kurbo::Point) -> bool {
        self.shapes.corner.is_some()
            || (session.editor.tool == ToolId::Select
                && corners::target(session, p, session.editor.view.zoom).is_some())
    }
}

/// The corner widgets of the one selected rect.
pub(super) fn draw_corner_widgets(painter: &Painter, origin: Pos2, session: &Session) {
    let view = session.editor.view;
    for (_, p) in corners::widgets(session, view.zoom) {
        let s = origin + to_egui(view::doc_to_screen(view, vextor_core::Point::new(p.x, p.y)));
        painter.circle_filled(s, WIDGET_RADIUS, theme::WHITE);
        painter.circle_stroke(s, WIDGET_RADIUS, Stroke::new(1.0, theme::LOGO_AMBER));
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Event, Modifiers, PointerButton, RawInput, Rect, Vec2};
    use vextor_core::{Geometry, NodeKind};

    use super::*;
    use crate::documents::Documents;
    use crate::new_doc::new_document;

    /// A canvas filling a 600×400 window, fed frame by frame with one
    /// egui context (it remembers the pointer between frames).
    struct Harness {
        ctx: egui::Context,
        doc: crate::documents::DocId,
        canvas: Canvas,
    }

    impl Harness {
        fn new() -> Self {
            let mut docs = Documents::default();
            let doc = docs.add(Session::new(new_document("x", 10.0, 10.0)), None);
            Self {
                ctx: egui::Context::default(),
                doc,
                canvas: Canvas::default(),
            }
        }

        /// One frame per event list, showing `s` at 100 % with no pan.
        fn run(&mut self, s: &mut Session, frames: Vec<Vec<Event>>) {
            for events in frames {
                let input = RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))),
                    events,
                    ..Default::default()
                };
                s.needs_fit = false;
                s.editor.view = vextor_core::View::default();
                let (canvas, doc) = (&mut self.canvas, self.doc);
                self.ctx
                    .run_ui(input, |ui| {
                        egui::CentralPanel::no_frame().show(ui, |ui| {
                            let _ = canvas.ui(ui, doc, s);
                        });
                    })
                    .textures_delta
                    .clear();
            }
        }
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn a_drag_on_the_canvas_draws_with_the_shape_tool() {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        s.editor.tool = ToolId::Ellipse;
        let mut h = Harness::new();
        let (a, b) = (Pos2::new(50.0, 60.0), Pos2::new(150.0, 120.0));
        h.run(
            &mut s,
            vec![
                vec![Event::PointerMoved(a)],
                vec![button(a, true)],
                vec![Event::PointerMoved(Pos2::new(100.0, 90.0))],
                vec![Event::PointerMoved(b)],
                vec![button(b, false)],
                vec![],
            ],
        );
        let [id] = s.selected()[..] else {
            panic!("one shape drawn and selected: {:?}", s.selected());
        };
        let NodeKind::Shape(shape) = &s.doc.nodes[&id].kind else {
            panic!("shape");
        };
        let Geometry::Ellipse { center, rx, ry } = shape.geometry else {
            panic!("ellipse, got {:?}", shape.geometry);
        };
        assert_eq!((center.x, center.y, rx, ry), (100.0, 90.0, 50.0, 30.0));
        assert_eq!(s.editor.tool, ToolId::Select);
        assert!(!h.canvas.is_drawing());
    }

    #[test]
    fn drawing_and_rounding_do_not_render_the_document_each_frame() {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        s.editor.tool = ToolId::Rect;
        let mut harness = Harness::new();
        let (start, end) = (Pos2::new(50.0, 60.0), Pos2::new(250.0, 220.0));
        let shown = |c: &Canvas| c.shown.map(|sh| sh.revision);
        harness.run(
            &mut s,
            vec![vec![Event::PointerMoved(start)], vec![button(start, true)]],
        );
        let at_press = shown(&harness.canvas);
        for x in [110.0, 120.0, 130.0, 140.0, 150.0] {
            let at = Pos2::new(x, 120.0);
            harness.run(&mut s, vec![vec![Event::PointerMoved(at)]]);
        }
        assert!(
            harness.canvas.drag_layers.is_some(),
            "drawn from the layers"
        );
        assert_eq!(
            shown(&harness.canvas),
            at_press,
            "no document render while drawing"
        );
        harness.run(
            &mut s,
            vec![
                vec![Event::PointerMoved(end)],
                vec![button(end, false)],
                vec![],
            ],
        );
        assert!(harness.canvas.drag_layers.is_none());
        assert_eq!(
            shown(&harness.canvas),
            Some(s.revision()),
            "one sharp render on release"
        );

        // The drawn rect is selected: round it from its top-left widget.
        let widget = Pos2::new(62.0, 72.0);
        harness.run(
            &mut s,
            vec![
                vec![Event::PointerMoved(widget)],
                vec![button(widget, true)],
            ],
        );
        let at_press = shown(&harness.canvas);
        for step in [2.0, 4.0, 6.0] {
            let at = Pos2::new(62.0 + step, 72.0 + step);
            harness.run(&mut s, vec![vec![Event::PointerMoved(at)]]);
        }
        assert!(harness.canvas.drag_layers.is_some());
        assert_eq!(
            shown(&harness.canvas),
            at_press,
            "no document render while rounding"
        );
        let up = Pos2::new(68.0, 78.0);
        harness.run(&mut s, vec![vec![button(up, false)], vec![]]);
        assert_eq!(s.edit_menu().undo, Some(crate::corners::LABEL));
        assert_eq!(shown(&harness.canvas), Some(s.revision()));
    }

    #[test]
    fn a_click_opens_the_size_dialog_and_enter_places_the_shape() {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        s.editor.tool = ToolId::Rect;
        let mut h = Harness::new();
        let at = Pos2::new(40.0, 30.0);
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        h.run(
            &mut s,
            vec![
                vec![Event::PointerMoved(at)],
                vec![button(at, true)],
                vec![button(at, false)],
                vec![],
            ],
        );
        assert!(h.canvas.is_drawing(), "the dialog is open");
        assert_eq!(s.edit_menu().undo, None);
        h.run(&mut s, vec![vec![enter], vec![]]);
        assert!(!h.canvas.is_drawing());
        let [id] = s.selected()[..] else {
            panic!("placed and selected");
        };
        let NodeKind::Shape(shape) = &s.doc.nodes[&id].kind else {
            panic!("shape");
        };
        assert_eq!(
            shape.geometry,
            Geometry::Rect {
                rect: vextor_core::Rect::new(40.0, 30.0, 100.0, 100.0),
                radius: vextor_core::CornerRadii::default(),
            }
        );
    }
}
