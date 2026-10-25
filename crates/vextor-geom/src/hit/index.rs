//! A spatial index over the hittable leaves (spec 14): the outlines are
//! built once per document revision and bucketed in a uniform grid, so a
//! pointer move tests only the leaves near it instead of every node.

use kurbo::{Point, Rect};
use vextor_core::{Document, NodeId};

use super::{Leaf, leaves};
use crate::text::Fonts;

/// Most grid cells along one side; a leaf over many cells is listed in
/// each, so the grid stays coarse.
const MAX_SIDE: usize = 128;

/// The hittable leaves of one document state, by grid cell. Build it again
/// when the document (or its fonts) changes.
pub struct HitIndex {
    /// Bottom → top, as drawn.
    leaves: Vec<Leaf>,
    /// The area the grid covers; leaves outside it go in `outside`.
    area: Rect,
    cols: usize,
    rows: usize,
    cell: (f64, f64),
    /// Leaf indices per cell, ascending.
    cells: Vec<Vec<u32>>,
    /// Leaves too big or too far out for the grid, ascending.
    outside: Vec<u32>,
}

impl HitIndex {
    /// Indexes every hittable leaf of `doc`.
    #[must_use]
    pub fn build(doc: &Document, fonts: &Fonts) -> Self {
        let leaves = leaves(doc, fonts);
        let boxes: Vec<Rect> = leaves.iter().map(Leaf::reach_box).collect();
        let area = boxes
            .iter()
            .copied()
            .filter(Rect::is_finite)
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        // About one leaf per cell.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )] // a small positive count
        let side = ((leaves.len() as f64).sqrt().ceil() as usize).clamp(1, MAX_SIDE);
        let (cols, rows) = (side, side);
        #[allow(clippy::cast_precision_loss)] // ≤ MAX_SIDE
        let cell = (
            (area.width() / cols as f64).max(f64::MIN_POSITIVE),
            (area.height() / rows as f64).max(f64::MIN_POSITIVE),
        );
        let mut index = Self {
            leaves,
            area,
            cols,
            rows,
            cell,
            cells: vec![Vec::new(); cols * rows],
            outside: Vec::new(),
        };
        for (i, b) in boxes.iter().enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            match index.span(*b) {
                Some((c0, r0, c1, r1)) => {
                    for r in r0..=r1 {
                        for c in c0..=c1 {
                            index.cells[r * cols + c].push(i);
                        }
                    }
                }
                None => index.outside.push(i),
            }
        }
        index
    }

    /// The topmost object with a leaf under `p`, within `tolerance`
    /// document px of its outline (same answer as [`super::pick`]).
    #[must_use]
    pub fn pick(&self, p: Point, tolerance: f64) -> Option<NodeId> {
        let near = Rect::from_center_size(p, (2.0 * tolerance, 2.0 * tolerance));
        let mut candidates = self.candidates(near);
        candidates.sort_unstable();
        candidates
            .iter()
            .rev()
            .map(|&i| &self.leaves[i as usize])
            .find(|leaf| leaf.hit(p, tolerance))
            .map(|leaf| leaf.object)
    }

    /// The objects a marquee `r` touches, bottom → top (same answer as
    /// [`super::touching`]). Leaves are kept in paint order, so the objects
    /// come out in it without sorting the whole document.
    #[must_use]
    pub fn touching(&self, r: Rect) -> Vec<NodeId> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<NodeId> = Vec::new();
        for i in self.candidates(r) {
            let leaf = &self.leaves[i as usize];
            if !seen.contains(&leaf.object) && leaf.touches(r) {
                seen.insert(leaf.object);
                out.push(leaf.object);
            }
        }
        out
    }

    /// The leaves whose boxes may touch `r`, each once, unordered.
    fn candidates(&self, r: Rect) -> Vec<u32> {
        let mut out = self.outside.clone();
        if let Some((c0, r0, c1, r1)) = self.span(r) {
            for row in r0..=r1 {
                for col in c0..=c1 {
                    out.extend_from_slice(&self.cells[row * self.cols + col]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The cells `r` covers, as (first col, first row, last col, last
    /// row); `None` when it misses the grid or is not finite.
    fn span(&self, r: Rect) -> Option<(usize, usize, usize, usize)> {
        if !r.is_finite() || !self.area.is_finite() {
            return None;
        }
        let a = self.area;
        if r.x1 < a.x0 || r.x0 > a.x1 || r.y1 < a.y0 || r.y0 > a.y1 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        let at = |v: f64, origin: f64, size: f64, n: usize| {
            (((v - origin) / size).floor().max(0.0) as usize).min(n - 1)
        };
        Some((
            at(r.x0, a.x0, self.cell.0, self.cols),
            at(r.y0, a.y0, self.cell.1, self.rows),
            at(r.x1, a.x0, self.cell.0, self.cols),
            at(r.y1, a.y0, self.cell.1, self.rows),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vextor_core::{Command, CornerRadii, Geometry, Node, NodeKind, Shape, Style};

    /// `n` filled 30 px squares on a grid 40 px apart, and one huge square
    /// under all of them.
    fn grid(n: u32) -> Document {
        let mut doc = Document::new("grid");
        let layer = doc.layers[0];
        let square = |doc: &mut Document, x: f64, y: f64, side: f64| {
            Node::new(
                doc.alloc_node_id(),
                NodeKind::Shape(Shape {
                    geometry: Geometry::Rect {
                        rect: vextor_core::Rect::new(x, y, side, side),
                        radius: CornerRadii::default(),
                    },
                    style: Style::default(),
                }),
            )
        };
        let mut nodes = vec![square(&mut doc, -100.0, -100.0, 5000.0)];
        for i in 0..n {
            let (x, y) = (f64::from(i % 25) * 40.0, f64::from(i / 25) * 40.0);
            nodes.push(square(&mut doc, x, y, 30.0));
        }
        nodes.reverse(); // inserted top first: the huge one ends at the bottom
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("inserts");
        doc
    }

    #[test]
    fn the_index_picks_what_the_full_scan_picks() {
        let doc = grid(500);
        let fonts = Fonts::default();
        let index = HitIndex::build(&doc, &fonts);
        for y in (-120..900).step_by(7) {
            for x in (-120..1100).step_by(7) {
                let p = Point::new(f64::from(x), f64::from(y));
                assert_eq!(
                    index.pick(p, 2.0),
                    super::super::pick(&doc, &fonts, p, 2.0),
                    "at {p:?}"
                );
            }
        }
    }

    #[test]
    fn the_index_finds_what_a_full_marquee_finds() {
        let doc = grid(500);
        let fonts = Fonts::default();
        let index = HitIndex::build(&doc, &fonts);
        for (x0, y0, x1, y1) in [
            (5.0, 5.0, 35.0, 35.0),
            (-200.0, -200.0, -150.0, -150.0),
            (100.0, 100.0, 530.0, 260.0),
            (-500.0, -500.0, 6000.0, 6000.0),
        ] {
            let r = Rect::new(x0, y0, x1, y1);
            assert_eq!(
                index.touching(r),
                super::super::touching(&doc, &fonts, r),
                "{r:?}"
            );
        }
    }

    #[test]
    fn an_empty_document_picks_nothing() {
        let doc = Document::new("empty");
        let index = HitIndex::build(&doc, &Fonts::default());
        assert_eq!(index.pick(Point::new(0.0, 0.0), 1.0), None);
    }
}
