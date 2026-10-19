//! Hit-testing and bounds: which object is under a point, which objects a
//! marquee touches, and where things are in document (world) space.
//!
//! "Objects" are what the selection tool picks: the direct children of a
//! layer (a group is picked whole). Hidden and locked nodes, and everything
//! inside them, are never hit. Clipped content is hit only inside its clip.

use std::collections::BTreeSet;

use kurbo::{Affine, BezPath, Line, ParamCurve, ParamCurveNearest, PathEl, Point, Rect, Shape};
use vextor_core::{Document, FillRule, Geometry, NodeId, NodeKind, Paint, Transform};

use crate::convert::to_bezpath;

/// Accuracy of the nearest-point search on curves, in document px.
const NEAREST_ACCURACY: f64 = 1e-3;

/// A model transform as a `kurbo` affine.
#[must_use]
pub fn to_affine(t: Transform) -> Affine {
    Affine::new(t.0)
}

/// A `kurbo` affine as a model transform.
#[must_use]
pub fn from_affine(a: Affine) -> Transform {
    Transform(a.as_coeffs())
}

/// One drawable leaf (shape or image) in world space.
struct Leaf {
    /// The selectable object it belongs to.
    object: NodeId,
    /// Outline in world space.
    outline: BezPath,
    /// Whether the inside counts (a visible fill, or an image).
    filled: bool,
    fill_rule: FillRule,
    /// Half the stroke width in world space; 0 without a visible stroke.
    half_stroke: f64,
    /// Clip outlines (world space) the leaf is drawn inside.
    clips: Vec<(BezPath, FillRule)>,
}

impl Leaf {
    fn hit(&self, p: Point, tolerance: f64) -> bool {
        let reach = self.half_stroke + tolerance;
        if !self
            .outline
            .bounding_box()
            .inflate(reach, reach)
            .contains(p)
        {
            return false;
        }
        if !self.clips.iter().all(|(c, rule)| inside(c, *rule, p)) {
            return false;
        }
        (self.filled && inside(&self.outline, self.fill_rule, p))
            || distance(&self.outline, p) <= reach
    }

    fn touches(&self, r: Rect) -> bool {
        let reach = self.half_stroke;
        if !self
            .outline
            .bounding_box()
            .inflate(reach, reach)
            .overlaps(r)
        {
            return false;
        }
        if !self.clips.iter().all(|(c, _)| c.bounding_box().overlaps(r)) {
            return false;
        }
        let edges = [
            Line::new((r.x0, r.y0), (r.x1, r.y0)),
            Line::new((r.x1, r.y0), (r.x1, r.y1)),
            Line::new((r.x1, r.y1), (r.x0, r.y1)),
            Line::new((r.x0, r.y1), (r.x0, r.y0)),
        ];
        self.outline.segments().any(|seg| {
            r.contains(seg.start()) || edges.iter().any(|e| !seg.intersect_line(*e).is_empty())
        }) || (self.filled && inside(&self.outline, self.fill_rule, r.center()))
    }
}

/// The topmost object with a leaf under `p` (document space), within
/// `tolerance` document px of its outline.
#[must_use]
pub fn pick(doc: &Document, p: Point, tolerance: f64) -> Option<NodeId> {
    leaves(doc)
        .iter()
        .rev()
        .find(|leaf| leaf.hit(p, tolerance))
        .map(|leaf| leaf.object)
}

/// The objects a marquee `r` (document space) touches, bottom → top.
#[must_use]
pub fn touching(doc: &Document, r: Rect) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = Vec::new();
    for leaf in leaves(doc).iter().filter(|leaf| leaf.touches(r)) {
        if !out.contains(&leaf.object) {
            out.push(leaf.object);
        }
    }
    doc.sorted_by_paint_order(&out)
}

/// Every selectable object (unlocked, visible child of an unlocked, visible
/// layer), bottom → top.
#[must_use]
pub fn selectable(doc: &Document) -> Vec<NodeId> {
    let clips: BTreeSet<NodeId> = doc
        .nodes
        .values()
        .filter_map(|n| n.container()?.clip)
        .collect();
    doc.layers
        .iter()
        .filter_map(|&l| doc.nodes.get(&l))
        .filter(|l| l.visible && !l.locked)
        .flat_map(|l| {
            l.container()
                .map(|c| c.children.clone())
                .unwrap_or_default()
        })
        .filter(|id| {
            doc.nodes
                .get(id)
                .is_some_and(|n| n.visible && !n.locked && !clips.contains(id))
        })
        .collect()
}

/// The geometric bounds of `id` and its visible descendants in document
/// space (strokes not included), or `None` when nothing visible is there.
#[must_use]
pub fn bounds(doc: &Document, id: NodeId) -> Option<Rect> {
    let parent = to_affine(doc.parent_transform(id));
    subtree_bounds(doc, id, parent, 0)
}

/// The union of [`bounds`] of `ids`.
#[must_use]
pub fn union_bounds(doc: &Document, ids: &[NodeId]) -> Option<Rect> {
    ids.iter()
        .filter_map(|&id| bounds(doc, id))
        .reduce(|a, b| a.union(b))
}

fn subtree_bounds(doc: &Document, id: NodeId, parent: Affine, depth: usize) -> Option<Rect> {
    let node = doc.nodes.get(&id)?;
    if !node.visible || depth > MAX_DEPTH {
        return None;
    }
    let world = parent * to_affine(node.transform);
    match &node.kind {
        NodeKind::Shape(s) => Some((world * to_bezpath(&s.geometry)).bounding_box()),
        NodeKind::Image(img) => Some((world * image_outline(img.rect)).bounding_box()),
        // Text has no extent until its layout exists (spec 05c, step 2).
        NodeKind::Text(_) => None,
        NodeKind::Layer(c) | NodeKind::Group(c) => {
            let inner = c
                .children
                .iter()
                .filter(|&&child| Some(child) != c.clip)
                .filter_map(|&child| subtree_bounds(doc, child, world, depth + 1))
                .reduce(|a, b| a.union(b))?;
            match c
                .clip
                .and_then(|clip| subtree_bounds(doc, clip, world, depth + 1))
            {
                Some(clip) => {
                    let r = inner.intersect(clip);
                    (r.width() >= 0.0 && r.height() >= 0.0).then_some(r)
                }
                None => Some(inner),
            }
        }
    }
}

/// Deepest nesting followed, matching the renderer's guard.
const MAX_DEPTH: usize = 64;

/// All hittable leaves, bottom → top.
fn leaves(doc: &Document) -> Vec<Leaf> {
    let mut out = Vec::new();
    for layer in doc.layers.iter().filter_map(|l| doc.nodes.get(l)) {
        if !layer.visible || layer.locked {
            continue;
        }
        let Some(c) = layer.container() else {
            continue;
        };
        let world = to_affine(layer.transform);
        let clips = clip_of(doc, c.clip, world);
        for &child in c.children.iter().filter(|&&ch| Some(ch) != c.clip) {
            collect(doc, child, child, world, &clips, 0, &mut out);
        }
    }
    out
}

fn collect(
    doc: &Document,
    id: NodeId,
    object: NodeId,
    parent: Affine,
    clips: &[(BezPath, FillRule)],
    depth: usize,
    out: &mut Vec<Leaf>,
) {
    let Some(node) = doc.nodes.get(&id) else {
        return;
    };
    if !node.visible || node.locked || depth > MAX_DEPTH {
        return;
    }
    let world = parent * to_affine(node.transform);
    match &node.kind {
        NodeKind::Shape(s) => {
            let fill_rule = match &s.geometry {
                Geometry::Path(p) => p.fill_rule,
                _ => FillRule::NonZero,
            };
            let filled =
                s.style.fill != Paint::None && !matches!(s.geometry, Geometry::Line { .. });
            let half_stroke = s
                .style
                .stroke
                .as_ref()
                .filter(|st| st.paint != Paint::None)
                .map_or(0.0, |st| st.width / 2.0 * scale_factor(world));
            out.push(Leaf {
                object,
                outline: world * to_bezpath(&s.geometry),
                filled,
                fill_rule,
                half_stroke,
                clips: clips.to_vec(),
            });
        }
        NodeKind::Image(img) => out.push(Leaf {
            object,
            outline: world * image_outline(img.rect),
            filled: true,
            fill_rule: FillRule::NonZero,
            half_stroke: 0.0,
            clips: clips.to_vec(),
        }),
        // Hit by its layout box from spec 05c, step 2.
        NodeKind::Text(_) => {}
        NodeKind::Layer(c) | NodeKind::Group(c) => {
            let mut inner = clips.to_vec();
            inner.extend(clip_of(doc, c.clip, world));
            for &child in c.children.iter().filter(|&&ch| Some(ch) != c.clip) {
                collect(doc, child, object, world, &inner, depth + 1, out);
            }
        }
    }
}

/// The world outline of a container's clip shape, if it has one.
fn clip_of(doc: &Document, clip: Option<NodeId>, container: Affine) -> Vec<(BezPath, FillRule)> {
    let Some(node) = clip.and_then(|id| doc.nodes.get(&id)) else {
        return Vec::new();
    };
    let NodeKind::Shape(s) = &node.kind else {
        return Vec::new();
    };
    let rule = match &s.geometry {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    };
    vec![(
        container * to_affine(node.transform) * to_bezpath(&s.geometry),
        rule,
    )]
}

fn image_outline(r: vextor_core::Rect) -> BezPath {
    kurbo::Rect::new(r.x, r.y, r.x + r.width, r.y + r.height).to_path(0.0)
}

/// How much `a` scales lengths on average (√|det|).
pub(crate) fn scale_factor(a: Affine) -> f64 {
    a.determinant().abs().sqrt()
}

/// Whether `p` is inside `path` by `rule`; open subpaths count as closed,
/// as SVG fills them.
fn inside(path: &BezPath, rule: FillRule, p: Point) -> bool {
    let w = closed(path).winding(p);
    match rule {
        FillRule::NonZero => w != 0,
        FillRule::EvenOdd => w % 2 != 0,
    }
}

/// `path` with every subpath closed.
fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if open {
                    out.push(PathEl::ClosePath);
                }
                open = true;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
    }
    if open {
        out.push(PathEl::ClosePath);
    }
    out
}

/// Shortest distance from `p` to the outline.
fn distance(path: &BezPath, p: Point) -> f64 {
    path.segments()
        .map(|seg| seg.nearest(p, NEAREST_ACCURACY).distance_sq)
        .fold(f64::INFINITY, f64::min)
        .sqrt()
}
