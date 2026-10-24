//! Builds the element tree of one export from the document, row by row of
//! the `svg-output` mapping table.
//!
//! Translations are pushed down the tree and baked into coordinates
//! (`x`, `cx`, path data), so an artboard's content reads in artboard
//! space with no `transform` on each element. Anything else (rotation,
//! scale) stays a `transform` on the element, as does a translation on an
//! object painted in user space (its gradient or pattern would move).

mod geometry;
mod paint;
mod text;

use std::collections::BTreeMap;

use vextor_core::{
    Container, Document, EPS, IdMode, Image, ImageSource, Node, NodeId, NodeKind, Paint, Resource,
    ResourceId, Rgba, Shape, Styling, SvgExportOptions, Transform, Vec2,
};
use vextor_geom::{Fonts, overlaps, visual_bounds};

use crate::element::{Element, extract_classes};
use crate::ids::{Ids, slug, unique_prefix};
use crate::number::Num;

pub(crate) use self::geometry::is_translation;

/// Deeper nesting than this is not written (guards the recursion; the
/// renderer draws no deeper either).
const MAX_DEPTH: usize = 256;

/// What one export covers.
pub(crate) struct Target {
    /// The exported area in document space; its top-left is the origin.
    pub area: kurbo::Rect,
    /// Roots in paint order, each with the transform from its parent's
    /// space to document space.
    pub roots: Vec<(NodeId, Transform)>,
    /// Painted first, over the whole area (an artboard's background).
    pub background: Option<Rgba>,
    /// Only a layer's objects touching this area are written (an
    /// artboard leaves out what lies on the others).
    pub within: Option<kurbo::Rect>,
    /// Hashed into the prefix of unique ids.
    pub key: String,
}

/// The `<svg>` element of `target`, without `xmlns` (standalone files add
/// it).
pub(crate) fn build(
    doc: &Document,
    fonts: &Fonts,
    opts: &SvgExportOptions,
    target: &Target,
) -> Element {
    let prefix = match opts.ids {
        IdMode::UniquePrefixed => unique_prefix(&target.key),
        IdMode::None | IdMode::Readable => String::new(),
    };
    let num = Num::new(opts.precision);
    let mut w = Writer {
        doc,
        fonts,
        opts,
        num,
        fine: num.at_least(3),
        ids: Ids::new(prefix),
        defs: Vec::new(),
        resources: BTreeMap::new(),
        patterns: 0,
        current: None,
        within: target.within,
    };
    if opts.current_color {
        w.current = paint::dominant(doc, &target.roots);
    }
    let origin = Transform::translate(-target.area.x0, -target.area.y0);
    let mut content = Vec::new();
    for &(id, parent) in &target.roots {
        w.node(id, parent.then(origin), 0, &mut content);
    }

    let mut svg = Element::new("svg");
    let (width, height) = (num.fmt(target.area.width()), num.fmt(target.area.height()));
    svg.set("viewBox", format!("0 0 {width} {height}"));
    if !opts.responsive {
        svg.set("width", width);
        svg.set("height", height);
    }
    if !w.defs.is_empty() {
        let mut defs = Element::new("defs");
        defs.children = std::mem::take(&mut w.defs);
        svg.children.push(defs);
    }
    if let Some(color) = target.background {
        let mut bg = Element::new("rect");
        bg.set("width", "100%");
        bg.set("height", "100%");
        w.solid(&mut bg, "fill", color, false);
        svg.children.push(bg);
    }
    svg.children.extend(content);
    if opts.styling == Styling::StyleElement
        && let Some(css) = extract_classes(&mut svg, w.ids.prefix())
    {
        let mut style = Element::new("style");
        style.text = Some(css);
        svg.children.insert(0, style);
    }
    svg
}

/// The state of one export while its tree is built.
struct Writer<'a> {
    doc: &'a Document,
    fonts: &'a Fonts,
    opts: &'a SvgExportOptions,
    /// Coordinates and lengths.
    num: Num,
    /// Values in 0..1: opacities, offsets, bounding-box units, angles.
    fine: Num,
    ids: Ids,
    /// `<defs>` content in order of first use.
    defs: Vec<Element>,
    /// Gradient and pattern ids written so far (`None`: paints nothing).
    resources: BTreeMap<ResourceId, Option<String>>,
    /// How many pattern tiles deep the writer is.
    patterns: usize,
    /// The dominant solid colour, written as `currentColor`.
    current: Option<[u8; 3]>,
    within: Option<kurbo::Rect>,
}

impl Writer<'_> {
    /// Writes node `id` (and its subtree) placed by `pending` (its parent's
    /// space → output space) into `out`; nothing for a hidden node.
    fn node(&mut self, id: NodeId, pending: Transform, depth: usize, out: &mut Vec<Element>) {
        let Some(node) = self.doc.nodes.get(&id) else {
            return;
        };
        if !node.visible || node.opacity <= EPS || depth > MAX_DEPTH {
            return;
        }
        let t = node.transform.then(pending);
        match &node.kind {
            NodeKind::Layer(c) | NodeKind::Group(c) => self.container(node, c, t, depth, out),
            NodeKind::Shape(s) => out.extend(self.shape(node, s, t)),
            NodeKind::Text(text) => out.extend(self.text(node, text, t)),
            NodeKind::Image(image) => out.push(self.image(node, image, t)),
        }
    }

    /// A group as `<g>`; a layer only when it carries an opacity or a clip
    /// (otherwise its objects are written in its place). Nothing when no
    /// child is visible.
    fn container(
        &mut self,
        node: &Node,
        c: &Container,
        t: Transform,
        depth: usize,
        out: &mut Vec<Element>,
    ) {
        let layer = matches!(node.kind, NodeKind::Layer(_));
        let wrap = !layer || node.opacity < 1.0 - EPS || c.clip.is_some();
        // A `<g>` keeps a rotation or scale; translations go to the children.
        let (own, inner) = if wrap && !is_translation(t) {
            (Some(t), Transform::IDENTITY)
        } else {
            (None, t)
        };
        let mut children = Vec::new();
        for &child in &c.children {
            if Some(child) == c.clip || (layer && !self.touches_area(child)) {
                continue;
            }
            self.node(child, inner, depth + 1, &mut children);
        }
        if children.is_empty() {
            return;
        }
        if !wrap {
            out.extend(children);
            return;
        }
        let mut g = Element::new("g");
        self.id_attr(&mut g, node);
        self.place(&mut g, own, node.opacity);
        if let Some(clip) = c.clip {
            // A clip that cannot be built hides the content, as SVG does.
            let Some(id) = self.clip_path(clip, inner) else {
                return;
            };
            g.set("clip-path", format!("url(#{id})"));
        }
        g.children = children;
        g.node = Some(node.id);
        out.push(g);
    }

    /// Whether a layer's object `id` lies (partly) in the exported area.
    fn touches_area(&self, id: NodeId) -> bool {
        match self.within {
            Some(area) if self.patterns == 0 => {
                visual_bounds(self.doc, self.fonts, id).is_some_and(|b| overlaps(b, area))
            }
            _ => true,
        }
    }

    fn shape(&mut self, node: &Node, s: &Shape, t: Transform) -> Option<Element> {
        let (offset, transform) = self.placement(t, s.style.paints());
        let mut el = geometry::element(&s.geometry, offset, self.num)?;
        self.id_attr(&mut el, node);
        self.place(&mut el, transform, node.opacity);
        let area = !matches!(s.geometry, vextor_core::Geometry::Line { .. });
        self.paint_attrs(&mut el, &s.style, area, geometry::fill_rule(&s.geometry));
        el.node = Some(node.id);
        Some(el)
    }

    fn image(&mut self, node: &Node, image: &Image, t: Transform) -> Element {
        let ImageSource::Embedded { mime, bytes } = &image.source;
        let (offset, transform) = self.placement(t, std::iter::empty());
        let mut el = Element::new("image");
        self.id_attr(&mut el, node);
        let r = image.rect;
        geometry::position(
            &mut el,
            ("x", "y"),
            r.x + offset.x,
            r.y + offset.y,
            self.num,
        );
        el.set("width", self.num.fmt(r.width));
        el.set("height", self.num.fmt(r.height));
        self.place(&mut el, transform, node.opacity);
        el.set("preserveAspectRatio", "none");
        let mime = match mime {
            vextor_core::ImageMime::Png => "png",
            vextor_core::ImageMime::Jpeg => "jpeg",
            vextor_core::ImageMime::Webp => "webp",
        };
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
        el.set("href", format!("data:image/{mime};base64,{data}"));
        el.node = Some(node.id);
        el
    }

    /// Where an object's coordinates go: a translation is baked in (the
    /// offset) unless a paint works in the object's own space; anything
    /// else becomes its `transform`.
    fn placement(
        &self,
        t: Transform,
        mut paints: impl Iterator<Item = Paint>,
    ) -> (Vec2, Option<Transform>) {
        let own_space = paints.any(|p| self.paints_in_user_space(p));
        if is_translation(t) && !own_space {
            let [.., e, f] = t.0;
            (Vec2::new(e, f), None)
        } else {
            (Vec2::new(0.0, 0.0), Some(t))
        }
    }

    /// Patterns and user-space gradients depend on the object's own
    /// coordinates.
    fn paints_in_user_space(&self, p: Paint) -> bool {
        match p {
            Paint::Pattern(_) => true,
            Paint::Gradient(id) => matches!(
                self.doc.resources.get(&id),
                Some(Resource::Gradient(g)) if g.units == vextor_core::GradientUnits::UserSpace
            ),
            Paint::None | Paint::Solid(_) | Paint::CurrentColor => false,
        }
    }

    /// The node's id first, when ids are on and it is named (not inside a
    /// pattern tile, which is written once per output anyway).
    fn id_attr(&mut self, el: &mut Element, node: &Node) {
        if self.opts.ids == IdMode::None || self.patterns > 0 {
            return;
        }
        if let Some(base) = node.name.as_deref().and_then(slug) {
            el.attrs.insert(0, ("id", self.ids.take(&base)));
        }
    }

    /// `transform` and `opacity`, when not the defaults.
    fn place(&self, el: &mut Element, transform: Option<Transform>, opacity: f64) {
        if let Some(value) = transform.and_then(|t| geometry::transform_attr(t, self.num)) {
            el.set("transform", value);
        }
        if opacity < 1.0 - EPS {
            el.set("opacity", self.fine.fmt(opacity));
        }
    }

    /// An id for a definition: from its name (unless ids are off), else
    /// `base-1`, `base-2`…
    fn def_id(&mut self, name: Option<&str>, base: &str) -> String {
        let name = name.filter(|_| self.opts.ids != IdMode::None);
        match name.and_then(slug) {
            Some(s) => self.ids.take(&s),
            None => self.ids.numbered(base),
        }
    }
}
