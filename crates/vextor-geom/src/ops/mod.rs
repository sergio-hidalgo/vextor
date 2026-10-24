//! Path operations (spec 11): booleans, divide, offset, outline stroke and
//! simplify, over areas in one coordinate space.
//!
//! Booleans run on polygons (`i_overlay`, robust on touching, identical
//! and self-intersecting input) and get their curves back from the inputs
//! afterwards (`recover`), so results stay curve-native.

mod modify;
mod polygon;
mod recover;

use i_overlay::core::overlay_rule::OverlayRule;
use kurbo::{BezPath, Point, Vec2};
use vextor_core::{EPS, FillRule, NodeType, Path};

pub use modify::{offset, outline_stroke, simplify};

use self::polygon::{Shapes, merged, normalise, overlay, ring_area};
use self::recover::Sources;
use crate::convert::bezpath_to_path;

/// A combining operation on areas listed bottom → top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    /// Everything any of them covers.
    Union,
    /// The bottom one minus all the others.
    Subtract,
    /// What all of them cover.
    Intersect,
    /// What an odd number of them cover.
    Exclude,
}

/// The region a filled outline covers, ready to combine with others.
/// Open subpaths count as closed.
#[derive(Clone, Debug, Default)]
pub struct Area {
    shapes: Shapes,
    sources: Sources,
}

impl Area {
    /// The region `outline` fills under `fill_rule`.
    #[must_use]
    pub fn new(outline: &BezPath, fill_rule: FillRule) -> Self {
        Self {
            shapes: normalise(outline, fill_rule),
            sources: Sources::of(outline),
        }
    }

    /// True when it covers nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Its size in square document px.
    #[must_use]
    pub fn size(&self) -> f64 {
        self.shapes
            .iter()
            .flatten()
            .map(|r| ring_area(r))
            .sum::<f64>()
            .abs()
    }

    /// True when `p` lies inside it.
    #[must_use]
    pub fn contains(&self, p: Point) -> bool {
        let winding: i32 = self
            .shapes
            .iter()
            .flatten()
            .map(|ring| winding(ring, p))
            .sum();
        winding != 0
    }

    /// Its outline, with the curves of the outlines it came from.
    #[must_use]
    pub fn to_bezpath(&self) -> BezPath {
        self.sources.to_bezpath(&self.shapes)
    }

    /// Its connected pieces, each an outer outline with its holes.
    #[must_use]
    pub fn pieces(&self) -> Vec<Self> {
        self.shapes
            .iter()
            .map(|shape| Self {
                shapes: vec![shape.clone()],
                sources: self.sources.clone(),
            })
            .collect()
    }

    /// `rule` applied to `self` and `other`.
    fn with(&self, other: &Self, rule: OverlayRule) -> Self {
        let mut sources = self.sources.clone();
        sources.extend(&other.sources);
        Self {
            shapes: overlay(&self.shapes, &other.shapes, rule),
            sources,
        }
    }

    /// Everything `areas` cover.
    #[must_use]
    pub fn union_all(areas: &[Self]) -> Self {
        let mut all = Self::default();
        for a in areas {
            all.shapes.extend(a.shapes.iter().cloned());
            all.sources.extend(&a.sources);
        }
        all.shapes = merged(&all.shapes);
        all
    }

    /// `op` applied to `areas`, bottom → top.
    #[must_use]
    pub fn combine(op: BooleanOp, areas: &[Self]) -> Self {
        let Some((bottom, rest)) = areas.split_first() else {
            return Self::default();
        };
        match op {
            BooleanOp::Union => Self::union_all(areas),
            BooleanOp::Subtract => bottom.with(&Self::union_all(rest), OverlayRule::Difference),
            BooleanOp::Intersect => rest
                .iter()
                .fold(bottom.clone(), |acc, a| acc.with(a, OverlayRule::Intersect)),
            BooleanOp::Exclude => rest
                .iter()
                .fold(bottom.clone(), |acc, a| acc.with(a, OverlayRule::Xor)),
        }
    }
}

/// Every region where `areas` (bottom → top) overlap differently, as
/// connected pieces, each with the index of the topmost area covering it.
#[must_use]
pub fn divide(areas: &[Area]) -> Vec<(Area, usize)> {
    let mut sources = Sources::default();
    let mut regions: Vec<(Shapes, usize)> = Vec::new();
    for (k, area) in areas.iter().enumerate() {
        sources.extend(&area.sources);
        let covered = merged(&regions.iter().flat_map(|r| r.0.clone()).collect());
        let mut next = Vec::with_capacity(regions.len() * 2 + 1);
        for (region, top) in regions {
            next.push((overlay(&region, &area.shapes, OverlayRule::Intersect), k));
            next.push((overlay(&region, &area.shapes, OverlayRule::Difference), top));
        }
        next.push((overlay(&area.shapes, &covered, OverlayRule::Difference), k));
        regions = next.into_iter().filter(|(r, _)| !r.is_empty()).collect();
    }
    regions
        .into_iter()
        .flat_map(|(region, top)| region.into_iter().map(move |shape| (shape, top)))
        .map(|(shape, top)| {
            let piece = Area {
                shapes: vec![shape],
                sources: sources.clone(),
            };
            (piece, top)
        })
        .collect()
}

/// An operation's result as an editable path: subpaths of a single node
/// dropped, nodes whose handles line up made smooth. `None` when nothing
/// is left.
#[must_use]
pub fn result_path(outline: &BezPath, fill_rule: FillRule) -> Option<Path> {
    let mut path = bezpath_to_path(outline, fill_rule);
    path.subpaths.retain(|sp| sp.nodes.len() >= 2);
    for node in path.subpaths.iter_mut().flat_map(|sp| sp.nodes.iter_mut()) {
        if let (Some(h_in), Some(h_out)) = (node.handle_in, node.handle_out) {
            let (a, b) = (Vec2::new(h_in.x, h_in.y), Vec2::new(h_out.x, h_out.y));
            let (la, lb) = (a.hypot(), b.hypot());
            if a.dot(b) < 0.0 && a.cross(b).abs() <= SMOOTH_SINE * la * lb {
                node.kind = if (la - lb).abs() <= EPS.max(la * 1e-6) {
                    NodeType::Symmetric
                } else {
                    NodeType::Smooth
                };
            }
        }
    }
    (!path.subpaths.is_empty()).then_some(path)
}

/// Handles this close to opposite (sine of the angle) make a smooth node.
const SMOOTH_SINE: f64 = 1e-3;

/// Winding number of `ring` around `point`.
fn winding(ring: &[[f64; 2]], point: Point) -> i32 {
    let (x, y) = (point.x, point.y);
    let mut turns = 0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        let side = (b[0] - a[0]) * (y - a[1]) - (x - a[0]) * (b[1] - a[1]);
        if a[1] <= y {
            if b[1] > y && side > 0.0 {
                turns += 1;
            }
        } else if b[1] <= y && side < 0.0 {
            turns -= 1;
        }
    }
    turns
}
