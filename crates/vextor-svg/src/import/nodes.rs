//! `usvg` tree → model nodes.
//!
//! `usvg` gives paths no transform of their own: an element's transform,
//! opacity or clip comes as a group around it. A group with no id, no clip
//! and one child is folded into that child (its transform and opacity
//! move onto it), which turns what the writer emits back into the objects
//! it came from.

use std::collections::{HashMap, HashSet};

use kurbo::BezPath;
use vextor_core::{
    ClipResource, ClipboardPayload, Container, FillRule, Geometry, MAX_NODES, Node, NodeId,
    NodeKind, Resource, ResourceId, Shape, Style, Transform, clean_name,
};
use vextor_geom::{Fonts, bezpath_to_path};

use super::guard::MAX_DEPTH;
use super::{Dropped, ImportError, Report};

/// The model objects of `tree`, ready to paste, and what was left out.
pub(super) fn map(
    tree: &super::parse::Tree,
    fonts: &Fonts,
) -> Result<(ClipboardPayload, Report), ImportError> {
    let mut m = Mapper {
        fonts,
        ids: &tree.ids,
        next: 1,
        count: 0,
        resources: Vec::new(),
        gradients: Vec::new(),
        patterns: HashMap::new(),
        nesting: 0,
        report: Report::default(),
    };
    for _ in 0..tree.rejected_images {
        m.report.drop(Dropped::Image);
    }
    let root = tree.parsed.root();
    let mut nodes = Vec::new();
    let roots = m.children(root, 0, &mut nodes);
    if m.count > MAX_NODES {
        return Err(ImportError::TooManyObjects);
    }
    if roots.is_empty() {
        return Err(ImportError::Empty);
    }
    let place = convert(root.transform());
    for node in &mut nodes {
        if roots.contains(&node.id) {
            node.transform = node.transform.then(place);
        }
    }
    m.report.objects = nodes.len();
    let payload = ClipboardPayload {
        nodes,
        resources: m.resources,
    };
    Ok((payload, m.report))
}

pub(super) struct Mapper<'a> {
    pub(super) fonts: &'a Fonts,
    ids: &'a HashSet<String>,
    /// Next local node or resource id (paste gives fresh ones).
    next: u64,
    /// Nodes made so far, pattern content included.
    count: usize,
    resources: Vec<ClipResource>,
    /// Gradients made so far, to share identical ones.
    pub(super) gradients: Vec<(vextor_core::Gradient, ResourceId)>,
    /// Patterns made so far, by `usvg` pattern.
    pub(super) patterns: HashMap<*const usvg::Pattern, Option<ResourceId>>,
    /// How many pattern tiles deep the mapper is.
    pub(super) nesting: usize,
    pub(super) report: Report,
}

impl Mapper<'_> {
    pub(super) fn node_id(&mut self) -> NodeId {
        self.count += 1;
        self.next += 1;
        NodeId(self.next)
    }

    pub(super) fn resource_id(&mut self) -> ResourceId {
        self.next += 1;
        ResourceId(self.next)
    }

    pub(super) fn add_resource(&mut self, id: ResourceId, resource: Resource, content: Vec<Node>) {
        self.resources.push(ClipResource {
            id,
            resource,
            content,
        });
    }

    /// The name `id` gives, when the file has that id (not one usvg made).
    pub(super) fn name(&self, id: &str) -> Option<String> {
        self.ids.contains(id).then(|| name_of(id)).flatten()
    }

    /// Records `resource`'s name when it has none yet (an identical copy
    /// usvg made was met first).
    pub(super) fn name_resource(&mut self, id: ResourceId, name: Option<String>) {
        let Some(name) = name else { return };
        for r in &mut self.resources {
            if r.id == id
                && let Resource::Gradient(g) = &mut r.resource
                && g.name.is_none()
            {
                g.name = Some(name.clone());
            }
        }
    }

    fn full(&self) -> bool {
        self.count > MAX_NODES
    }

    /// `g`'s children as nodes pushed to `out` (descendants first); the
    /// ids of the direct ones, bottom → top.
    pub(super) fn children(
        &mut self,
        g: &usvg::Group,
        depth: usize,
        out: &mut Vec<Node>,
    ) -> Vec<NodeId> {
        let mut ids = Vec::new();
        for child in g.children() {
            if self.full() || depth > MAX_DEPTH {
                break;
            }
            if let Some(id) = self.node(child, depth, out) {
                ids.push(id);
            }
        }
        ids
    }

    fn node(&mut self, n: &usvg::Node, depth: usize, out: &mut Vec<Node>) -> Option<NodeId> {
        match n {
            usvg::Node::Group(g) => self.group(g, depth, out),
            usvg::Node::Path(p) => self.path(p).map(|node| push(out, node)),
            usvg::Node::Image(i) => self.image(i, depth, out),
            usvg::Node::Text(t) => self.text(t, depth, out),
        }
    }

    pub(super) fn group(
        &mut self,
        g: &usvg::Group,
        depth: usize,
        out: &mut Vec<Node>,
    ) -> Option<NodeId> {
        if !g.filters().is_empty() {
            self.report.drop(Dropped::Filter);
        }
        if g.mask().is_some() {
            self.report.drop(Dropped::Mask);
        }
        if g.blend_mode() != usvg::BlendMode::Normal {
            self.report.drop(Dropped::BlendMode);
        }
        let mut children = self.children(g, depth + 1, out);
        if children.is_empty() {
            return None;
        }
        let transform = convert(g.transform());
        let opacity = f64::from(g.opacity().get());
        let clip = match g.clip_path() {
            Some(c) => {
                // A clip that keeps nothing hides everything, as in SVG.
                let shape = self.clip(c)?;
                let id = self.node_id();
                out.push(Node::new(id, NodeKind::Shape(shape)));
                children.insert(0, id);
                Some(id)
            }
            None => None,
        };
        let name = self.name(g.id());
        if clip.is_none()
            && let [only] = children[..]
            && let Some(node) = out.iter_mut().rev().find(|n| n.id == only)
            && (name.is_none() || node.name.is_none())
        {
            // usvg wraps an element in a group for its transform, opacity
            // (or, for an image, its id): that group is the element.
            node.transform = node.transform.then(transform);
            node.opacity = (node.opacity * opacity).clamp(0.0, 1.0);
            if node.name.is_none() {
                node.name = name;
            }
            if let NodeKind::Image(image) = &mut node.kind {
                super::image::fit_image(image, &mut node.transform);
            }
            return Some(only);
        }
        let id = self.node_id();
        let mut node = Node::new(id, NodeKind::Group(Container { children, clip }));
        node.name = name;
        node.transform = transform;
        node.opacity = opacity.clamp(0.0, 1.0);
        Some(push(out, node))
    }

    fn path(&mut self, p: &usvg::Path) -> Option<Node> {
        if p.fill().is_none() && p.stroke().is_none() {
            return None;
        }
        let bez = bezpath(p.data());
        let rule = p.fill().map_or(FillRule::NonZero, |f| fill_rule(f.rule()));
        let path = bezpath_to_path(&bez, rule);
        if path.subpaths.is_empty() {
            return None;
        }
        if p.paint_order() == usvg::PaintOrder::StrokeAndFill && p.stroke().is_some() {
            self.report.drop(Dropped::PaintOrder);
        }
        let bbox = rect(p.bounding_box());
        let style = Style {
            fill: p.fill().map_or(vextor_core::Paint::None, |f| {
                self.paint(f.paint(), f.opacity(), bbox)
            }),
            stroke: p.stroke().and_then(|s| self.stroke(s, bbox)),
        };
        let id = self.node_id();
        let mut node = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Path(path),
                style,
            }),
        );
        node.name = self.name(p.id());
        node.visible = p.is_visible();
        Some(node)
    }
}

/// The name an element's `id` gives, without the prefix the writer's
/// unique ids add (`vx-` and four hex digits), so a round trip does not
/// stack prefixes.
fn name_of(id: &str) -> Option<String> {
    let bare = id
        .strip_prefix("vx-")
        .and_then(|rest| {
            let (hex, rest) = rest.split_at_checked(4)?;
            let rest = rest.strip_prefix('-')?;
            (hex.bytes().all(|b| b.is_ascii_hexdigit()) && !rest.is_empty()).then_some(rest)
        })
        .unwrap_or(id);
    clean_name(bare)
}

/// Pushes `node`, returning its id.
pub(super) fn push(out: &mut Vec<Node>, node: Node) -> NodeId {
    let id = node.id;
    out.push(node);
    id
}

/// A `tiny-skia` path as a `kurbo` one (f32 → f64).
pub(super) fn bezpath(p: &usvg::tiny_skia_path::Path) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;
    let pt = |p: usvg::tiny_skia_path::Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
    let mut out = BezPath::new();
    for seg in p.segments() {
        match seg {
            PathSegment::MoveTo(p) => out.move_to(pt(p)),
            PathSegment::LineTo(p) => out.line_to(pt(p)),
            PathSegment::QuadTo(a, b) => out.quad_to(pt(a), pt(b)),
            PathSegment::CubicTo(a, b, c) => out.curve_to(pt(a), pt(b), pt(c)),
            PathSegment::Close => out.close_path(),
        }
    }
    out
}

/// A `usvg` transform as a model one.
pub(super) fn convert(t: usvg::Transform) -> Transform {
    let v = [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from);
    Transform(v)
}

pub(super) fn rect(r: usvg::Rect) -> kurbo::Rect {
    kurbo::Rect::new(
        f64::from(r.left()),
        f64::from(r.top()),
        f64::from(r.right()),
        f64::from(r.bottom()),
    )
}

pub(super) fn fill_rule(r: usvg::FillRule) -> FillRule {
    match r {
        usvg::FillRule::NonZero => FillRule::NonZero,
        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
    }
}
