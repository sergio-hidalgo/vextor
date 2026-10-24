//! Definitions objects refer to: clip paths and patterns.

use std::sync::Arc;

use kurbo::{Affine, BezPath};
use vextor_core::{FillRule, Geometry, Pattern, Rect, Resource, ResourceId, Shape, Style};
use vextor_geom::{Area, bezpath_to_path, to_affine};

use super::Dropped;
use super::guard::MAX_DEPTH;
use super::nodes::{Mapper, bezpath, convert, fill_rule};

/// Patterns inside pattern tiles deeper than this paint nothing (as the
/// writer and renderer).
const MAX_PATTERN_NESTING: usize = 4;

impl Mapper<'_> {
    /// One shape covering what `c` keeps, in the clipped group's child
    /// space; `None` when it keeps nothing.
    pub(super) fn clip(&mut self, c: &usvg::ClipPath) -> Option<Shape> {
        if c.clip_path().is_some() {
            self.report.drop(Dropped::NestedClip);
        }
        let mut parts = Vec::new();
        collect_outlines(c.root(), to_affine(convert(c.transform())), 0, &mut parts);
        let path = match parts.as_slice() {
            [] => return None,
            [(bez, rule)] => bezpath_to_path(bez, *rule),
            _ => {
                let areas: Vec<Area> = parts.iter().map(|(b, r)| Area::new(b, *r)).collect();
                bezpath_to_path(&Area::union_all(&areas).to_bezpath(), FillRule::NonZero)
            }
        };
        (!path.subpaths.is_empty()).then(|| Shape {
            geometry: Geometry::Path(path),
            style: Style::default(),
        })
    }

    /// The pattern resource for `p`, made on first use.
    pub(super) fn pattern(&mut self, p: &Arc<usvg::Pattern>) -> Option<ResourceId> {
        let key = Arc::as_ptr(p);
        if let Some(&done) = self.patterns.get(&key) {
            return done;
        }
        if self.nesting >= MAX_PATTERN_NESTING {
            return None;
        }
        // Known before the content is mapped: a tile may paint with it.
        let id = self.resource_id();
        self.patterns.insert(key, Some(id));
        self.nesting += 1;
        let mut content = Vec::new();
        let roots = self.children(p.root(), 1, &mut content);
        self.nesting -= 1;
        if roots.is_empty() {
            self.patterns.insert(key, None);
            return None;
        }
        let place = convert(p.root().transform());
        for node in &mut content {
            if roots.contains(&node.id) {
                node.transform = node.transform.then(place);
            }
        }
        let tile = p.rect();
        let pattern = Pattern {
            name: self.name(p.id()),
            tile: Rect::new(
                f64::from(tile.x()),
                f64::from(tile.y()),
                f64::from(tile.width()),
                f64::from(tile.height()),
            ),
            transform: convert(p.transform()),
            content: roots,
        };
        self.add_resource(id, Resource::Pattern(pattern), content);
        Some(id)
    }
}

/// Every filled outline under `g` (text as its outlines), placed by `t`.
fn collect_outlines(g: &usvg::Group, t: Affine, depth: usize, out: &mut Vec<(BezPath, FillRule)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let t = t * to_affine(convert(g.transform()));
    for child in g.children() {
        match child {
            usvg::Node::Group(g) => collect_outlines(g, t, depth + 1, out),
            usvg::Node::Path(p) => {
                let mut bez = bezpath(p.data());
                bez.apply_affine(t);
                let rule = p.fill().map_or(FillRule::NonZero, |f| fill_rule(f.rule()));
                out.push((bez, rule));
            }
            usvg::Node::Text(text) => collect_outlines(text.flattened(), t, depth + 1, out),
            usvg::Node::Image(_) => {}
        }
    }
}
