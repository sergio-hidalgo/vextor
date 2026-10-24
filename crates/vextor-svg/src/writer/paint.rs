//! Fill and stroke attributes, and the `<defs>` they refer to: gradients,
//! patterns and clip paths, each written once, in order of first use.

use std::collections::BTreeMap;

use vextor_core::{
    Document, EPS, FillRule, Gradient, GradientKind, GradientUnits, LineCap, LineJoin, NodeId,
    NodeKind, Paint, Resource, ResourceId, Rgba, Spread, Stroke, Style, Transform,
};

use super::{MAX_DEPTH, Writer, geometry, is_translation};
use crate::element::Element;

/// Patterns inside pattern tiles deeper than this paint nothing (the
/// renderer stops at the same depth).
const MAX_PATTERN_NESTING: usize = 4;

impl Writer<'_> {
    /// Fill (when the shape has an area) and stroke attributes.
    pub(super) fn paint_attrs(
        &mut self,
        el: &mut Element,
        style: &Style,
        area: bool,
        rule: FillRule,
    ) {
        if area {
            self.paint(el, "fill", style.fill);
            if rule == FillRule::EvenOdd {
                el.set("fill-rule", "evenodd");
            }
        }
        if let Some(stroke) = &style.stroke {
            self.stroke(el, stroke);
        }
    }

    /// `name` (`fill` or `stroke`) and its opacity, unless the SVG default.
    fn paint(&mut self, el: &mut Element, name: &'static str, paint: Paint) {
        let value = match paint {
            Paint::None => "none".to_owned(),
            Paint::CurrentColor => "currentColor".to_owned(),
            Paint::Solid(c) => return self.solid(el, name, c, true),
            Paint::Gradient(id) => reference(self.gradient(id)),
            Paint::Pattern(id) => reference(self.pattern(id)),
        };
        if !(name == "fill" && value == "#000" || name == "stroke" && value == "none") {
            el.set(name, value);
        }
    }

    /// A solid colour as `name` (`fill` or `stroke`), `currentColor` when it
    /// is the dominant one (and `current` allows it), and its alpha as
    /// `name-opacity`.
    pub(super) fn solid(&self, el: &mut Element, name: &'static str, c: Rgba, current: bool) {
        let value = if current && self.current == Some([c.r, c.g, c.b]) {
            "currentColor".to_owned()
        } else {
            hex(c)
        };
        if !(name == "fill" && value == "#000") {
            el.set(name, value);
        }
        if c.a < 255 {
            let opacity = if name == "fill" {
                "fill-opacity"
            } else {
                "stroke-opacity"
            };
            el.set(opacity, self.fine.fmt(f64::from(c.a) / 255.0));
        }
    }

    fn stroke(&mut self, el: &mut Element, st: &Stroke) {
        if st.paint == Paint::None || st.width <= EPS {
            return;
        }
        self.paint(el, "stroke", st.paint);
        if !self.num.same(st.width, 1.0) {
            el.set("stroke-width", self.num.fmt(st.width));
        }
        match st.cap {
            LineCap::Butt => {}
            LineCap::Round => el.set("stroke-linecap", "round"),
            LineCap::Square => el.set("stroke-linecap", "square"),
        }
        match st.join {
            LineJoin::Miter => {
                if !self.num.same(st.miter_limit, 4.0) {
                    el.set("stroke-miterlimit", self.num.fmt(st.miter_limit));
                }
            }
            LineJoin::Round => el.set("stroke-linejoin", "round"),
            LineJoin::Bevel => el.set("stroke-linejoin", "bevel"),
        }
        // SVG draws an invalid or all-zero dash list solid, like the canvas.
        let dashed = st.dash.iter().all(|d| *d >= 0.0) && st.dash.iter().any(|d| *d > EPS);
        if dashed {
            let dash: Vec<String> = st.dash.iter().map(|d| self.num.fmt(*d)).collect();
            el.set("stroke-dasharray", dash.join(" "));
            if self.num.units(st.dash_offset) != 0 {
                el.set("stroke-dashoffset", self.num.fmt(st.dash_offset));
            }
        }
    }

    /// The id of gradient `id`'s definition, written on first use.
    fn gradient(&mut self, id: ResourceId) -> Option<String> {
        if let Some(written) = self.resources.get(&id) {
            return written.clone();
        }
        let Some(Resource::Gradient(g)) = self.doc.resources.get(&id) else {
            self.resources.insert(id, None);
            return None;
        };
        let def = self.def_id(g.name.as_deref(), "gradient");
        let el = self.gradient_element(g, &def);
        self.defs.push(el);
        self.resources.insert(id, Some(def.clone()));
        Some(def)
    }

    fn gradient_element(&self, g: &Gradient, id: &str) -> Element {
        let bbox = g.units == GradientUnits::ObjectBoundingBox;
        // Bounding-box units run 0..1: they need the fine decimals.
        let n = if bbox { self.fine } else { self.num };
        let mut el;
        // In bounding-box units the defaults are a left-to-right line and a
        // circle filling the box; user space has no useful defaults.
        let coord = |el: &mut Element, name: &'static str, v: f64, default: f64| {
            if !bbox || !n.same(v, default) {
                el.set(name, n.fmt(v));
            }
        };
        match g.kind {
            GradientKind::Linear { start, end } => {
                el = Element::new("linearGradient");
                el.set("id", id);
                coord(&mut el, "x1", start.x, 0.0);
                coord(&mut el, "y1", start.y, 0.0);
                coord(&mut el, "x2", end.x, 1.0);
                coord(&mut el, "y2", end.y, 0.0);
            }
            GradientKind::Radial {
                center,
                radius,
                focal,
            } => {
                el = Element::new("radialGradient");
                el.set("id", id);
                coord(&mut el, "cx", center.x, 0.5);
                coord(&mut el, "cy", center.y, 0.5);
                coord(&mut el, "r", radius, 0.5);
                if let Some(f) = focal.filter(|f| !n.same(f.x, center.x) || !n.same(f.y, center.y))
                {
                    el.set("fx", n.fmt(f.x));
                    el.set("fy", n.fmt(f.y));
                }
            }
        }
        if !bbox {
            el.set("gradientUnits", "userSpaceOnUse");
        }
        match g.spread {
            Spread::Pad => {}
            Spread::Reflect => el.set("spreadMethod", "reflect"),
            Spread::Repeat => el.set("spreadMethod", "repeat"),
        }
        for stop in &g.stops {
            let mut s = Element::new("stop");
            if self.fine.units(stop.offset) != 0 {
                s.set("offset", self.fine.fmt(stop.offset));
            }
            let c = stop.color;
            // `stop-color` defaults to black.
            if [c.r, c.g, c.b] != [0, 0, 0] {
                s.set("stop-color", hex(c));
            }
            if c.a < 255 {
                s.set("stop-opacity", self.fine.fmt(f64::from(c.a) / 255.0));
            }
            el.children.push(s);
        }
        el
    }

    /// The id of pattern `id`'s definition, written on first use with its
    /// tile's content.
    fn pattern(&mut self, id: ResourceId) -> Option<String> {
        if let Some(written) = self.resources.get(&id) {
            return written.clone();
        }
        let Some(Resource::Pattern(p)) = self.doc.resources.get(&id) else {
            self.resources.insert(id, None);
            return None;
        };
        if self.patterns >= MAX_PATTERN_NESTING {
            return None;
        }
        let def = self.def_id(p.name.as_deref(), "pattern");
        // Known before the content is written: a tile may paint with it.
        self.resources.insert(id, Some(def.clone()));
        let slot = self.defs.len();
        self.defs.push(Element::new("pattern"));
        self.patterns += 1;
        let mut content = Vec::new();
        for &root in &p.content {
            self.node(root, Transform::IDENTITY, 0, &mut content);
        }
        self.patterns -= 1;
        let mut el = Element::new("pattern");
        el.set("id", def.clone());
        geometry::position(&mut el, ("x", "y"), p.tile.x, p.tile.y, self.num);
        el.set("width", self.num.fmt(p.tile.width));
        el.set("height", self.num.fmt(p.tile.height));
        el.set("patternUnits", "userSpaceOnUse");
        if let Some(t) = geometry::transform_attr(p.transform, self.num) {
            el.set("patternTransform", t);
        }
        el.children = content;
        self.defs[slot] = el;
        Some(def)
    }

    /// A `<clipPath>` of shape `clip`, placed by `pending` like the
    /// container's children; its id, or `None` when it is not a drawable
    /// shape.
    pub(super) fn clip_path(&mut self, clip: NodeId, pending: Transform) -> Option<String> {
        let node = self.doc.nodes.get(&clip)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        let t = node.transform.then(pending);
        let mut shape = if is_translation(t) {
            let [.., e, f] = t.0;
            geometry::element(&s.geometry, vextor_core::Vec2::new(e, f), self.num)?
        } else {
            let mut el =
                geometry::element(&s.geometry, vextor_core::Vec2::new(0.0, 0.0), self.num)?;
            if let Some(value) = geometry::transform_attr(t, self.num) {
                el.set("transform", value);
            }
            el
        };
        if geometry::fill_rule(&s.geometry) == FillRule::EvenOdd {
            shape.set("clip-rule", "evenodd");
        }
        let id = self.ids.numbered("clip");
        let mut el = Element::new("clipPath");
        el.set("id", id.clone());
        el.children.push(shape);
        self.defs.push(el);
        Some(id)
    }
}

/// `url(#id)`, or `none` when the resource paints nothing.
fn reference(id: Option<String>) -> String {
    id.map_or_else(|| "none".to_owned(), |id| format!("url(#{id})"))
}

/// `#rgb` when lossless, else `#rrggbb` (alpha goes in `*-opacity`).
pub(crate) fn hex(c: Rgba) -> String {
    let short = |v: u8| v >> 4 == v & 0xf;
    if short(c.r) && short(c.g) && short(c.b) {
        format!("#{:x}{:x}{:x}", c.r & 0xf, c.g & 0xf, c.b & 0xf)
    } else {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }
}

/// The solid colour most fills and strokes under `roots` use (ties: the
/// first one met in paint order), for the `currentColor` option.
pub(super) fn dominant(doc: &Document, roots: &[(NodeId, Transform)]) -> Option<[u8; 3]> {
    let mut counts: BTreeMap<[u8; 3], (usize, usize)> = BTreeMap::new();
    let mut stack: Vec<(NodeId, usize)> = roots.iter().rev().map(|&(id, _)| (id, 0)).collect();
    let mut seen = 0;
    while let Some((id, depth)) = stack.pop() {
        let Some(node) = doc.nodes.get(&id) else {
            continue;
        };
        if !node.visible || depth > MAX_DEPTH {
            continue;
        }
        if let Some(c) = node.container() {
            let children = c.children.iter().filter(|&&ch| Some(ch) != c.clip);
            stack.extend(children.rev().map(|&ch| (ch, depth + 1)));
            continue;
        }
        for paint in node.paints() {
            if let Paint::Solid(c) = paint {
                let entry = counts.entry([c.r, c.g, c.b]).or_insert((0, seen));
                entry.0 += 1;
                seen += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.cmp(&a.1.1)))
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_use_the_short_form_when_lossless() {
        assert_eq!(hex(Rgba::rgb(255, 0, 0)), "#f00");
        assert_eq!(hex(Rgba::rgb(0x12, 0x34, 0x56)), "#123456");
        assert_eq!(hex(Rgba::BLACK), "#000");
    }
}
