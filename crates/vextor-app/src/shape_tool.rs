//! The Rectangle, Ellipse, Polygon, Star and Line tools (spec 06), as a
//! state machine over a [`Session`] fed with document-space points, so it
//! is tested without a window. The geometry itself is `vextor_geom::drawn`.
//!
//! The shape appears on the first move that gives it a size and follows
//! the pointer until release: each move puts the insert back with the new
//! geometry, so the whole drag is one "Draw …" undo step. A click without
//! a drag asks for the size instead ([`Release::Clicked`]).

use kurbo::Point;
use vextor_core::{
    Command, Document, Geometry, MAX_POLYGON_SIDES, MAX_STAR_POINTS, Node, NodeId, NodeKind, Paint,
    Shape, Style, ToolId, Transform,
};
use vextor_geom::{DrawMods, ShapeKind, ShapeSize, drawn, sized};

use crate::edit_ops;
use crate::session::Session;

/// Sides of a new polygon.
pub const DEFAULT_SIDES: u32 = 6;
/// Points of a new star.
pub const DEFAULT_POINTS: u32 = 5;
/// Inner radius of a new star, as a share of the outer radius.
pub const DEFAULT_INNER: f64 = 0.5;
/// How much ←/→ change a star's inner radius while drawing.
pub const INNER_STEP: f64 = 0.05;
/// The inner radius share ←/→ stay within.
pub const INNER_RANGE: std::ops::RangeInclusive<f64> = 0.05..=0.95;

/// A key that changes the shape being drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawKey {
    /// ↑: one more side or point.
    More,
    /// ↓: one fewer.
    Fewer,
    /// →: fatter star (inner radius grows).
    Fatter,
    /// ←: thinner star.
    Thinner,
}

/// How a press-drag-release ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Release {
    /// A shape was drawn and selected.
    Drawn(NodeId),
    /// No drag: ask for the size of a shape placed at this point.
    Clicked(Point),
    /// Nothing was under way.
    Nothing,
}

/// A drag in progress.
#[derive(Debug)]
struct Drag {
    tool: ToolId,
    start: Point,
    end: Point,
    mods: DrawMods,
    /// The shape once the drag has given it a size.
    id: Option<NodeId>,
    /// The document when the press began, for the canvas's fast path.
    before: Box<Document>,
}

/// The shape tools' state: the drag under way, and the polygon and star
/// options, kept for the next shape.
#[derive(Debug)]
pub struct ShapeTool {
    drag: Option<Drag>,
    pub sides: u32,
    pub points: u32,
    pub inner: f64,
}

impl Default for ShapeTool {
    fn default() -> Self {
        Self {
            drag: None,
            sides: DEFAULT_SIDES,
            points: DEFAULT_POINTS,
            inner: DEFAULT_INNER,
        }
    }
}

/// The undo label of drawing with `tool`.
#[must_use]
pub fn label(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Ellipse => "Draw Ellipse",
        ToolId::Polygon => "Draw Polygon",
        ToolId::Star => "Draw Star",
        ToolId::Line => "Draw Line",
        _ => "Draw Rectangle",
    }
}

/// The style a new shape drawn with `tool` gets: the session's default
/// style; a line, which has no inside, gets its stroke (1 px black when
/// there is none) and no fill.
#[must_use]
pub fn new_style(s: &Session, tool: ToolId) -> Style {
    let style = s.editor.default_style.clone();
    if tool == ToolId::Line {
        Style {
            fill: Paint::None,
            stroke: Some(style.stroke.unwrap_or_default()),
        }
    } else {
        style
    }
}

impl ShapeTool {
    /// True while a drag is under way.
    #[must_use]
    pub fn is_drawing(&self) -> bool {
        self.drag.is_some()
    }

    /// While drawing: the press-time document and the shape drawn so far
    /// (none until the drag has a size).
    #[must_use]
    pub fn preview(&self) -> Option<(&Document, Option<NodeId>)> {
        self.drag.as_ref().map(|d| (&*d.before, d.id))
    }

    /// What `tool` draws with the current options.
    #[must_use]
    pub fn kind(&self, tool: ToolId) -> ShapeKind {
        match tool {
            ToolId::Ellipse => ShapeKind::Ellipse,
            ToolId::Polygon => ShapeKind::Polygon { sides: self.sides },
            ToolId::Star => ShapeKind::Star {
                points: self.points,
                inner: self.inner,
            },
            ToolId::Line => ShapeKind::Line,
            _ => ShapeKind::Rect,
        }
    }

    /// The size dialog's starting values for `tool`.
    #[must_use]
    pub fn default_size(&self, tool: ToolId) -> ShapeSize {
        match tool {
            ToolId::Ellipse => ShapeSize::Ellipse {
                width: 100.0,
                height: 100.0,
            },
            ToolId::Polygon => ShapeSize::Polygon {
                radius: 50.0,
                sides: self.sides,
            },
            ToolId::Star => ShapeSize::Star {
                outer: 50.0,
                inner: 50.0 * self.inner,
                points: self.points,
            },
            ToolId::Line => ShapeSize::Line {
                length: 100.0,
                angle: 0.0,
            },
            _ => ShapeSize::Rect {
                width: 100.0,
                height: 100.0,
            },
        }
    }

    /// The pointer pressed at `p` with shape tool `tool`.
    ///
    /// # Errors
    /// A message for the user when there is no unlocked layer to draw in.
    pub fn press(&mut self, s: &mut Session, tool: ToolId, p: Point) -> Result<(), String> {
        self.drag = None;
        if edit_ops::paste_target(s).is_none() {
            return Err("Draw failed: every layer is hidden or locked".to_owned());
        }
        s.begin_gesture();
        self.drag = Some(Drag {
            tool,
            start: p,
            end: p,
            mods: DrawMods::default(),
            id: None,
            before: Box::new(s.doc.clone()),
        });
        Ok(())
    }

    /// The pointer moved to `p` with the button held and `mods`.
    ///
    /// # Errors
    /// A message for the user when the shape is refused.
    pub fn drag_to(&mut self, s: &mut Session, p: Point, mods: DrawMods) -> Result<(), String> {
        let Some(d) = &mut self.drag else {
            return Ok(());
        };
        if d.end == p && d.mods == mods {
            return Ok(());
        }
        d.end = p;
        d.mods = mods;
        self.update(s)
    }

    /// A key pressed while drawing: ↑/↓ sides or points, ←/→ the star's
    /// inner radius. Returns true when the key was used.
    ///
    /// # Errors
    /// A message for the user when the shape is refused.
    pub fn key(&mut self, s: &mut Session, key: DrawKey) -> Result<bool, String> {
        let Some(tool) = self.drag.as_ref().map(|d| d.tool) else {
            return Ok(false);
        };
        let count = |n: u32, max: u32| match key {
            DrawKey::More => (n + 1).min(max),
            DrawKey::Fewer => n.saturating_sub(1).max(3),
            _ => n,
        };
        let used = match (tool, key) {
            (ToolId::Polygon, DrawKey::More | DrawKey::Fewer) => {
                self.sides = count(self.sides, MAX_POLYGON_SIDES);
                true
            }
            (ToolId::Star, DrawKey::More | DrawKey::Fewer) => {
                self.points = count(self.points, MAX_STAR_POINTS);
                true
            }
            (ToolId::Star, DrawKey::Fatter | DrawKey::Thinner) => {
                let step = if key == DrawKey::Fatter {
                    INNER_STEP
                } else {
                    -INNER_STEP
                };
                self.inner = (self.inner + step).clamp(*INNER_RANGE.start(), *INNER_RANGE.end());
                true
            }
            _ => false,
        };
        if used {
            self.update(s)?;
        }
        Ok(used)
    }

    /// The pointer button was released: selects the shape drawn and
    /// returns to the selection tool, or reports a click.
    pub fn release(&mut self, s: &mut Session) -> Release {
        let Some(d) = self.drag.take() else {
            return Release::Nothing;
        };
        s.end_gesture();
        match d.id {
            Some(id) => {
                s.select(vec![id]);
                s.editor.tool = ToolId::Select;
                Release::Drawn(id)
            }
            None => Release::Clicked(d.start),
        }
    }

    /// `Esc` while drawing: the shape is taken back, as if never drawn.
    pub fn cancel(&mut self, s: &mut Session) {
        let Some(d) = self.drag.take() else {
            return;
        };
        s.end_gesture();
        if d.id.is_some() {
            s.undo();
        }
    }

    /// Puts the shape where the drag says: inserts it, or puts the insert
    /// back with the new geometry. A drag back to no size removes it.
    fn update(&mut self, s: &mut Session) -> Result<(), String> {
        let Some(d) = &self.drag else {
            return Ok(());
        };
        let geometry = drawn(self.kind(d.tool), d.start, d.end, d.mods);
        let (tool, old) = (d.tool, d.id);
        if old.is_some() {
            // The top step is this drag's own insert.
            s.undo();
        }
        let id = match geometry {
            Some(g) => Some(insert(s, tool, g, old)?),
            None => None,
        };
        if let Some(d) = &mut self.drag {
            d.id = id;
        }
        Ok(())
    }
}

/// Places a shape of `size` at `p` (the size dialog's answer), selected,
/// in one "Draw …" step, and returns to the selection tool.
///
/// # Errors
/// A message for the user when the size is empty or the shape is refused.
pub fn place(
    s: &mut Session,
    tool: ToolId,
    size: ShapeSize,
    p: Point,
    centred: bool,
) -> Result<NodeId, String> {
    let geometry = sized(size, p, centred).ok_or("Draw failed: the size must be above zero")?;
    let id = insert(s, tool, geometry, None)?;
    s.select(vec![id]);
    s.editor.tool = ToolId::Select;
    Ok(id)
}

/// Inserts a shape drawn with `tool` with `geometry` in document space, on
/// top of the paste target, under id `reuse` or a fresh one.
fn insert(
    s: &mut Session,
    tool: ToolId,
    geometry: Geometry,
    reuse: Option<NodeId>,
) -> Result<NodeId, String> {
    let (parent, index) = edit_ops::paste_target(s)
        .ok_or_else(|| "Draw failed: every layer is hidden or locked".to_owned())?;
    let id = reuse.unwrap_or_else(|| s.doc.alloc_node_id());
    let mut node = Node::new(
        id,
        NodeKind::Shape(Shape {
            geometry,
            style: new_style(s, tool),
        }),
    );
    // Drawn in document space: undo the parent's transform.
    node.transform = s
        .doc
        .world_transform(parent)
        .invert()
        .unwrap_or(Transform::IDENTITY);
    let insert = Command::Insert {
        parent: Some(parent),
        index,
        nodes: vec![node],
    };
    s.apply(Command::Batch {
        label: label(tool),
        commands: vec![insert],
    })
    .map_err(|e| format!("Draw failed: {e}"))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use vextor_core::{Document, Rect, Rgba, Stroke};

    use super::*;
    use crate::new_doc::new_document;

    fn session() -> Session {
        Session::new(new_document("a", 400.0, 200.0))
    }

    fn geometry(s: &Session, id: NodeId) -> &Geometry {
        match &s.doc.nodes[&id].kind {
            NodeKind::Shape(shape) => &shape.geometry,
            other => panic!("shape, got {other:?}"),
        }
    }

    fn style(s: &Session, id: NodeId) -> &Style {
        match &s.doc.nodes[&id].kind {
            NodeKind::Shape(shape) => &shape.style,
            other => panic!("shape, got {other:?}"),
        }
    }

    /// Press at `a`, move through `moves`, release.
    fn draw(s: &mut Session, tool: ToolId, a: (f64, f64), moves: &[(f64, f64)]) -> Release {
        let mut t = ShapeTool::default();
        t.press(s, tool, Point::new(a.0, a.1)).expect("presses");
        for &(x, y) in moves {
            t.drag_to(s, Point::new(x, y), DrawMods::default())
                .expect("drags");
        }
        t.release(s)
    }

    fn drawn_id(r: Release) -> NodeId {
        match r {
            Release::Drawn(id) => id,
            other => panic!("drawn, got {other:?}"),
        }
    }

    #[test]
    fn dragging_draws_a_rect_in_one_undo_step() {
        let mut s = session();
        let r = draw(
            &mut s,
            ToolId::Rect,
            (10.0, 20.0),
            &[(30.0, 30.0), (60.0, 50.0), (110.0, 60.0)],
        );
        let id = drawn_id(r);
        assert_eq!(
            geometry(&s, id),
            &Geometry::Rect {
                rect: Rect::new(10.0, 20.0, 100.0, 40.0),
                radius: vextor_core::CornerRadii::default(),
            }
        );
        assert_eq!(s.selected(), vec![id]);
        assert_eq!(s.editor.tool, ToolId::Select, "back to the selection tool");
        assert_eq!(s.edit_menu().undo, Some("Draw Rectangle"));
        assert!(s.undo());
        assert!(!s.doc.nodes.contains_key(&id));
        assert_eq!(s.edit_menu().undo, None, "the whole drag was one step");
    }

    #[test]
    fn each_tool_draws_its_own_kind() {
        let cases = [
            (ToolId::Ellipse, "Draw Ellipse"),
            (ToolId::Polygon, "Draw Polygon"),
            (ToolId::Star, "Draw Star"),
            (ToolId::Line, "Draw Line"),
        ];
        for (tool, undo) in cases {
            let mut s = session();
            let id = drawn_id(draw(&mut s, tool, (10.0, 10.0), &[(50.0, 40.0)]));
            let ok = matches!(
                (tool, geometry(&s, id)),
                (ToolId::Ellipse, Geometry::Ellipse { .. })
                    | (
                        ToolId::Polygon,
                        Geometry::Polygon {
                            sides: DEFAULT_SIDES,
                            ..
                        }
                    )
                    | (
                        ToolId::Star,
                        Geometry::Star {
                            points: DEFAULT_POINTS,
                            ..
                        }
                    )
                    | (ToolId::Line, Geometry::Line { .. })
            );
            assert!(ok, "{tool:?}: {:?}", geometry(&s, id));
            assert_eq!(s.edit_menu().undo, Some(undo));
        }
    }

    #[test]
    fn shift_and_alt_reach_the_geometry() {
        let mut s = session();
        let mut t = ShapeTool::default();
        t.press(&mut s, ToolId::Ellipse, Point::new(100.0, 100.0))
            .expect("presses");
        let both = DrawMods {
            constrain: true,
            from_centre: true,
        };
        t.drag_to(&mut s, Point::new(130.0, 110.0), both)
            .expect("drags");
        let id = drawn_id(t.release(&mut s));
        let Geometry::Ellipse { center, rx, ry } = geometry(&s, id) else {
            panic!("ellipse");
        };
        assert_eq!((center.x, center.y, *rx, *ry), (100.0, 100.0, 30.0, 30.0));
    }

    #[test]
    fn a_click_asks_for_the_size_and_records_nothing() {
        let mut s = session();
        s.editor.tool = ToolId::Rect;
        let r = draw(&mut s, ToolId::Rect, (40.0, 30.0), &[]);
        assert_eq!(r, Release::Clicked(Point::new(40.0, 30.0)));
        assert_eq!(s.edit_menu().undo, None);
        assert_eq!(s.editor.tool, ToolId::Rect, "kept until the dialog answers");
    }

    #[test]
    fn a_drag_back_to_no_size_draws_nothing() {
        let mut s = session();
        let r = draw(
            &mut s,
            ToolId::Rect,
            (40.0, 30.0),
            &[(80.0, 60.0), (40.0, 30.0)],
        );
        assert_eq!(r, Release::Clicked(Point::new(40.0, 30.0)));
        assert_eq!(s.edit_menu().undo, None);
        assert_eq!(s.doc.nodes.len(), session().doc.nodes.len());
    }

    #[test]
    fn escape_takes_the_shape_back() {
        let mut s = session();
        let before = s.doc.clone();
        let mut t = ShapeTool::default();
        t.press(&mut s, ToolId::Star, Point::new(0.0, 0.0))
            .expect("presses");
        t.drag_to(&mut s, Point::new(50.0, 50.0), DrawMods::default())
            .expect("drags");
        t.cancel(&mut s);
        assert!(!t.is_drawing());
        assert_eq!(s.doc, before);
        assert_eq!(t.release(&mut s), Release::Nothing);
    }

    #[test]
    fn arrows_change_sides_points_and_inner_radius_while_drawing() {
        let mut s = session();
        let mut t = ShapeTool::default();
        t.press(&mut s, ToolId::Polygon, Point::new(0.0, 0.0))
            .expect("presses");
        t.drag_to(&mut s, Point::new(50.0, 50.0), DrawMods::default())
            .expect("drags");
        assert!(t.key(&mut s, DrawKey::More).expect("keys"));
        assert!(t.key(&mut s, DrawKey::More).expect("keys"));
        assert!(
            !t.key(&mut s, DrawKey::Fatter).expect("keys"),
            "no inner radius"
        );
        let id = drawn_id(t.release(&mut s));
        assert!(matches!(
            geometry(&s, id),
            Geometry::Polygon { sides: 8, .. }
        ));
        assert_eq!(t.sides, 8, "kept for the next polygon");

        t.press(&mut s, ToolId::Star, Point::new(0.0, 0.0))
            .expect("presses");
        t.drag_to(
            &mut s,
            Point::new(0.0, 40.0),
            DrawMods {
                constrain: false,
                from_centre: true,
            },
        )
        .expect("drags");
        for _ in 0..5 {
            t.key(&mut s, DrawKey::Fewer).expect("keys");
        }
        t.key(&mut s, DrawKey::Thinner).expect("keys");
        let id = drawn_id(t.release(&mut s));
        let Geometry::Star {
            outer,
            inner,
            points,
            ..
        } = geometry(&s, id)
        else {
            panic!("star");
        };
        assert_eq!(*points, 3, "never below three");
        assert!((inner / outer - 0.45).abs() < 1e-9);
    }

    #[test]
    fn keys_do_nothing_when_not_drawing() {
        let mut s = session();
        let mut t = ShapeTool::default();
        assert!(!t.key(&mut s, DrawKey::More).expect("keys"));
        assert_eq!(t.sides, DEFAULT_SIDES);
    }

    #[test]
    fn new_shapes_take_the_default_style_and_lines_its_stroke() {
        let mut s = session();
        s.editor.default_style = Style {
            fill: Paint::Solid(Rgba::rgb(0x1e, 0x90, 0xff)),
            stroke: None,
        };
        let rect = drawn_id(draw(&mut s, ToolId::Rect, (0.0, 0.0), &[(20.0, 20.0)]));
        assert_eq!(style(&s, rect), &s.editor.default_style.clone());
        let line = drawn_id(draw(&mut s, ToolId::Line, (0.0, 0.0), &[(20.0, 20.0)]));
        assert_eq!(style(&s, line).fill, Paint::None);
        assert_eq!(style(&s, line).stroke, Some(Stroke::default()));
    }

    #[test]
    fn drawing_inside_a_moved_layer_keeps_the_pointer_position() {
        let mut doc: Document = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        doc.nodes.get_mut(&layer).expect("layer").transform =
            Transform([1.0, 0.0, 0.0, 1.0, 100.0, 0.0]);
        let mut s = Session::new(doc);
        let id = drawn_id(draw(&mut s, ToolId::Rect, (10.0, 10.0), &[(30.0, 30.0)]));
        let world = s.doc.world_transform(id);
        assert_eq!(
            (world.0[4], world.0[5]),
            (0.0, 0.0),
            "drawn where the pointer was"
        );
    }

    #[test]
    fn no_unlocked_layer_refuses_to_draw() {
        let mut s = session();
        let layer = s.doc.layers[0];
        s.doc.nodes.get_mut(&layer).expect("layer").locked = true;
        let mut t = ShapeTool::default();
        assert!(t.press(&mut s, ToolId::Rect, Point::new(0.0, 0.0)).is_err());
        assert!(!t.is_drawing());
    }

    #[test]
    fn the_size_dialog_places_a_selected_shape_in_one_step() {
        let mut s = session();
        let t = ShapeTool::default();
        let size = t.default_size(ToolId::Star);
        let id = place(&mut s, ToolId::Star, size, Point::new(80.0, 60.0), false).expect("places");
        assert!(matches!(geometry(&s, id), Geometry::Star { points: 5, .. }));
        assert_eq!(s.selected(), vec![id]);
        assert_eq!(s.edit_menu().undo, Some("Draw Star"));
        let empty = ShapeSize::Rect {
            width: 0.0,
            height: 10.0,
        };
        assert!(place(&mut s, ToolId::Rect, empty, Point::new(0.0, 0.0), false).is_err());
    }
}
