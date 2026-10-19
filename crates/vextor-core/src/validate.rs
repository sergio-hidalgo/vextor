//! Document invariants (`specs/data-model.md`, *Invariants*).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::document::Document;
use crate::ids::{NodeId, ResourceId};
use crate::math::{Point, Rect};
use crate::node::{CornerRadii, Geometry, Node, NodeKind};
use crate::resource::{GradientKind, Resource};
use crate::style::{Paint, Stroke, Style};
use crate::text::{FONT_WEIGHTS, MAX_FONT_FAMILY_CHARS, MAX_TEXT_CHARS, Text};

/// Most sides of a polygon; more looks like a circle.
pub const MAX_POLYGON_SIDES: u32 = 20;

/// Most points of a star.
pub const MAX_STAR_POINTS: u32 = 20;

/// One broken invariant, described for a developer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation(pub String);

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Document {
    /// Checks every invariant and returns all violations found.
    ///
    /// # Errors
    /// The list of [`Violation`]s when the document is not valid.
    pub fn validate(&self) -> Result<(), Vec<Violation>> {
        let mut c = Checker {
            doc: self,
            out: Vec::new(),
        };
        c.ids();
        c.tree();
        for node in self.nodes.values() {
            c.node(node);
        }
        for (id, r) in &self.resources {
            c.resource(*id, r);
        }
        c.settings();
        if c.out.is_empty() { Ok(()) } else { Err(c.out) }
    }
}

struct Checker<'a> {
    doc: &'a Document,
    out: Vec<Violation>,
}

fn nonneg(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

fn radii_ok(r: CornerRadii) -> bool {
    [r.tl, r.tr, r.br, r.bl].into_iter().all(nonneg)
}

fn rect_ok(r: Rect) -> bool {
    r.is_finite() && r.width >= 0.0 && r.height >= 0.0
}

impl Checker<'_> {
    fn fail(&mut self, msg: String) {
        self.out.push(Violation(msg));
    }

    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.fail(msg());
        }
    }

    fn ids(&mut self) {
        let next = self.doc.next_id;
        for (key, node) in &self.doc.nodes {
            self.check(*key == node.id, || {
                format!("node stored under {key:?} has id {:?}", node.id)
            });
            self.check(key.0 < next, || {
                format!("node {key:?} is not below the allocator ({next})")
            });
        }
        for key in self.doc.resources.keys() {
            self.check(key.0 < next, || {
                format!("resource {key:?} is not below the allocator ({next})")
            });
        }
        let mut seen = BTreeSet::new();
        for a in &self.doc.artboards {
            self.check(a.id.0 < next, || {
                format!("artboard {:?} is not below the allocator ({next})", a.id)
            });
            self.check(seen.insert(a.id), || {
                format!("artboard {:?} is listed twice", a.id)
            });
            self.check(rect_ok(a.rect), || {
                format!("artboard {:?} has an invalid rect", a.id)
            });
        }
    }

    /// Every referenced id exists, every node has exactly one holder, only
    /// layers are top level, everything is reachable (no cycles).
    fn tree(&mut self) {
        let doc = self.doc;
        let mut holders: BTreeMap<NodeId, usize> = BTreeMap::new();
        let mut roots = Vec::new();
        for &id in &doc.layers {
            *holders.entry(id).or_default() += 1;
            roots.push(id);
            let is_layer = doc.nodes.get(&id).is_some_and(Node::is_layer);
            self.check(is_layer, || {
                format!("top-level {id:?} is missing or not a layer")
            });
        }
        for r in doc.resources.values() {
            for &id in r.content() {
                *holders.entry(id).or_default() += 1;
                roots.push(id);
            }
        }
        for node in doc.nodes.values() {
            let Some(c) = node.container() else { continue };
            for &child in &c.children {
                *holders.entry(child).or_default() += 1;
            }
            if let Some(clip) = c.clip {
                let is_shape = matches!(
                    doc.nodes.get(&clip).map(|n| &n.kind),
                    Some(NodeKind::Shape(_))
                );
                self.check(c.children.contains(&clip) && is_shape, || {
                    format!(
                        "clip {clip:?} of {:?} is not one of its shape children",
                        node.id
                    )
                });
            }
        }
        for (&id, &count) in &holders {
            match doc.nodes.get(&id) {
                None => self.fail(format!("{id:?} is referenced but does not exist")),
                Some(n) => {
                    self.check(count == 1, || format!("{id:?} has {count} parents"));
                    let top = doc.layers.contains(&id);
                    self.check(!n.is_layer() || top, || format!("layer {id:?} is nested"));
                }
            }
        }
        let mut reached = BTreeSet::new();
        for root in roots {
            reached.extend(doc.subtree(root));
        }
        for id in doc.nodes.keys() {
            self.check(reached.contains(id), || {
                format!("{id:?} is unreachable (orphan or cycle)")
            });
        }
    }

    fn node(&mut self, n: &Node) {
        let id = n.id;
        self.check(
            n.opacity.is_finite() && (0.0..=1.0).contains(&n.opacity),
            || format!("{id:?} opacity {} is outside 0..=1", n.opacity),
        );
        self.check(n.transform.is_finite(), || {
            format!("{id:?} transform is not finite")
        });
        match &n.kind {
            NodeKind::Layer(_) | NodeKind::Group(_) => {}
            NodeKind::Shape(s) => {
                self.geometry(id, &s.geometry);
                self.style(id, &s.style);
            }
            NodeKind::Image(img) => self.check(rect_ok(img.rect), || {
                format!("{id:?} image rect is invalid")
            }),
            NodeKind::Text(t) => self.text(id, t),
        }
    }

    fn text(&mut self, id: NodeId, t: &Text) {
        let chars = t.content.chars().count();
        self.check(chars <= MAX_TEXT_CHARS, || {
            format!("{id:?} text has {chars} characters, more than {MAX_TEXT_CHARS}")
        });
        let family = t.font.family.trim();
        self.check(
            !family.is_empty() && family.chars().count() <= MAX_FONT_FAMILY_CHARS,
            || format!("{id:?} font family is empty or too long"),
        );
        self.check(FONT_WEIGHTS.contains(&t.font.weight), || {
            format!("{id:?} font weight {} is outside 100..=900", t.font.weight)
        });
        let metrics_ok = t.size.is_finite()
            && t.size > 0.0
            && t.line_height.is_finite()
            && t.line_height > 0.0
            && t.letter_spacing.is_finite();
        self.check(metrics_ok, || {
            format!("{id:?} text size, line height or letter spacing is invalid")
        });
        self.style(id, &t.style);
        if let Some(f) = &t.frame {
            let ok = f.padding.into_iter().all(nonneg) && radii_ok(f.radius);
            self.check(ok, || format!("{id:?} text frame has invalid numbers"));
            self.paint(id, f.fill);
            if let Some(s) = &f.stroke {
                self.stroke(id, s);
            }
        }
    }

    fn geometry(&mut self, id: NodeId, g: &Geometry) {
        let ok = match g {
            Geometry::Rect { rect, radius } => rect_ok(*rect) && radii_ok(*radius),
            Geometry::Ellipse { center, rx, ry } => {
                center.is_finite() && nonneg(*rx) && nonneg(*ry)
            }
            Geometry::Polygon {
                center,
                radius,
                sides,
                corner_radius,
            } => {
                center.is_finite()
                    && nonneg(*radius)
                    && (3..=MAX_POLYGON_SIDES).contains(sides)
                    && nonneg(*corner_radius)
            }
            Geometry::Star {
                center,
                outer,
                inner,
                points,
                corner_radius,
            } => {
                center.is_finite()
                    && nonneg(*outer)
                    && nonneg(*inner)
                    && (3..=MAX_STAR_POINTS).contains(points)
                    && nonneg(*corner_radius)
            }
            Geometry::Line { from, to } => from.is_finite() && to.is_finite(),
            Geometry::Path(p) => {
                self.check(!p.subpaths.is_empty(), || {
                    format!("{id:?} path has no subpaths")
                });
                for (s, sp) in p.subpaths.iter().enumerate() {
                    self.check(!sp.nodes.is_empty(), || {
                        format!("{id:?} subpath {s} is empty")
                    });
                    self.check(!sp.closed || sp.nodes.len() >= 2, || {
                        format!("{id:?} subpath {s} is closed with fewer than two nodes")
                    });
                    for (i, node) in sp.nodes.iter().enumerate() {
                        self.check(node.is_finite() && node.corner_radius >= 0.0, || {
                            format!("{id:?} node {s}/{i} has invalid numbers")
                        });
                        self.check(node.satisfies_type(), || {
                            format!("{id:?} node {s}/{i} handles break its {:?} type", node.kind)
                        });
                    }
                }
                true
            }
        };
        self.check(ok, || {
            format!("{id:?} geometry has invalid numbers or counts")
        });
    }

    fn style(&mut self, id: NodeId, style: &Style) {
        for paint in style.paints() {
            self.paint(id, paint);
        }
        if let Some(s) = &style.stroke {
            self.stroke_numbers(id, s);
        }
    }

    /// A stroke outside a [`Style`]: its paint and its numbers.
    fn stroke(&mut self, id: NodeId, s: &Stroke) {
        self.paint(id, s.paint);
        self.stroke_numbers(id, s);
    }

    fn stroke_numbers(&mut self, id: NodeId, s: &Stroke) {
        let ok = nonneg(s.width)
            && s.miter_limit.is_finite()
            && s.miter_limit >= 1.0
            && s.dash.iter().copied().all(nonneg)
            && s.dash_offset.is_finite();
        self.check(ok, || format!("{id:?} stroke has invalid numbers"));
    }

    fn paint(&mut self, id: NodeId, paint: Paint) {
        let (rid, want_gradient) = match paint {
            Paint::Gradient(r) => (r, true),
            Paint::Pattern(r) => (r, false),
            Paint::None | Paint::Solid(_) | Paint::CurrentColor => return,
        };
        let ok = match self.doc.resources.get(&rid) {
            Some(Resource::Gradient(_)) => want_gradient,
            Some(Resource::Pattern(_)) => !want_gradient,
            None => false,
        };
        self.check(ok, || {
            format!("{id:?} paint refers to missing or wrong-kind resource {rid:?}")
        });
    }

    fn resource(&mut self, id: ResourceId, r: &Resource) {
        match r {
            Resource::Gradient(g) => {
                let stops_ok = g.stops.len() >= 2
                    && g.stops
                        .iter()
                        .all(|s| s.offset.is_finite() && (0.0..=1.0).contains(&s.offset))
                    && g.stops.windows(2).all(|w| w[0].offset <= w[1].offset);
                self.check(stops_ok, || {
                    format!("{id:?} needs ≥ 2 stops with sorted offsets in 0..=1")
                });
                let kind_ok = match &g.kind {
                    GradientKind::Linear { start, end } => start.is_finite() && end.is_finite(),
                    GradientKind::Radial {
                        center,
                        radius,
                        focal,
                    } => {
                        center.is_finite() && nonneg(*radius) && focal.is_none_or(Point::is_finite)
                    }
                };
                self.check(kind_ok, || format!("{id:?} gradient geometry is invalid"));
            }
            Resource::Pattern(p) => {
                let ok = rect_ok(p.tile) && p.transform.is_finite();
                self.check(ok, || {
                    format!("{id:?} pattern tile or transform is invalid")
                });
                for &c in &p.content {
                    let not_layer = !self.doc.nodes.get(&c).is_some_and(Node::is_layer);
                    self.check(not_layer, || {
                        format!("{id:?} pattern content {c:?} is a layer")
                    });
                }
            }
        }
    }

    fn settings(&mut self) {
        let g = &self.doc.grid;
        let ok = g.size.is_finite()
            && g.size > 0.0
            && g.subdivisions >= 1
            && g.snap.angle_step_deg.is_finite()
            && g.snap.angle_step_deg > 0.0
            && nonneg(g.snap.tolerance_px);
        self.check(ok, || "grid or snap settings are invalid".to_owned());
        let precision = self.doc.export.precision;
        self.check(precision <= 6, || {
            format!("export precision {precision} is above 6")
        });
    }
}
