//! What node editing shows on the edited path: its handles, the round
//! widgets of selected corners, and which nodes a marquee covers.

use kurbo::{Affine, Point, Rect, Vec2};
use vextor_core::{NodeId, PathNodeRef, Side};
use vextor_geom::{node_corner, to_kurbo};

use super::WIDGET_INSET;
use crate::path_ops::{self, edited, selected_nodes, world};
use crate::session::Session;

/// One shown handle: its node, side, and anchor and tip in document space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShownHandle {
    pub node: PathNodeRef,
    pub side: Side,
    pub anchor: Point,
    pub tip: Point,
    /// A selected node's own handle (or the selected segment's), as
    /// opposed to a neighbour's handle facing it: drawn brighter, on top,
    /// and grabbed first where they overlap.
    pub own: bool,
}

/// The smaller scale of `a` (document px per local px).
pub(super) fn scale(a: Affine) -> f64 {
    let [m0, m1, m2, m3, _, _] = a.as_coeffs();
    m0.hypot(m1).min(m2.hypot(m3)).max(vextor_core::EPS)
}

/// The nodes of `shape` inside `r` (document space).
pub(super) fn nodes_in(s: &Session, shape: NodeId, r: Rect) -> Vec<PathNodeRef> {
    let Some(path) = s.path_of(shape) else {
        return Vec::new();
    };
    let w = world(s, shape);
    let mut out = Vec::new();
    for (subpath, sp) in path.subpaths.iter().enumerate() {
        for (index, n) in sp.nodes.iter().enumerate() {
            if r.contains(w * to_kurbo(n.anchor)) {
                out.push(PathNodeRef {
                    shape,
                    subpath,
                    index,
                });
            }
        }
    }
    out
}

/// The handles shown on the edited path: those of the selected nodes
/// (`own`) and of their neighbours facing them, or the two of the selected
/// segment (`own`). Neighbours come first, so own handles draw on top.
#[must_use]
pub fn handles(s: &Session) -> Vec<ShownHandle> {
    let Some(shape) = edited(s) else {
        return Vec::new();
    };
    let Some(path) = s.path_of(shape) else {
        return Vec::new();
    };
    let at = |subpath, index| PathNodeRef {
        shape,
        subpath,
        index,
    };
    let mut wanted: Vec<(PathNodeRef, Side, bool)> = Vec::new();
    if let Some((_, subpath, index)) = path_ops::segment_target(s)
        && let Some(sp) = path.subpaths.get(subpath)
    {
        wanted.push((at(subpath, index), Side::Out, true));
        wanted.push((at(subpath, (index + 1) % sp.nodes.len()), Side::In, true));
    }
    for r in selected_nodes(s) {
        let Some(sp) = path.subpaths.get(r.subpath) else {
            continue;
        };
        let n = sp.nodes.len();
        wanted.push((r, Side::In, true));
        wanted.push((r, Side::Out, true));
        if sp.closed || r.index > 0 {
            wanted.push((at(r.subpath, (r.index + n - 1) % n), Side::Out, false));
        }
        if sp.closed || r.index + 1 < n {
            wanted.push((at(r.subpath, (r.index + 1) % n), Side::In, false));
        }
    }
    // A handle both own and a neighbour's (two adjacent selected nodes)
    // counts as own.
    let own = |node, side| {
        wanted
            .iter()
            .any(|&(n, sd, o)| o && n == node && sd == side)
    };
    let mut wanted: Vec<(PathNodeRef, Side, bool)> = wanted
        .iter()
        .map(|&(node, side, _)| (node, side, own(node, side)))
        .collect();
    wanted.sort_by_key(|&(_, _, own)| own);
    let w = world(s, shape);
    let mut out: Vec<ShownHandle> = Vec::new();
    for (node, side, own) in wanted {
        let Some(n) = s.path_node(node) else {
            continue;
        };
        let h = match side {
            Side::In => n.handle_in,
            Side::Out => n.handle_out,
        };
        let Some(h) = h else {
            continue;
        };
        let anchor = to_kurbo(n.anchor);
        let shown = ShownHandle {
            node,
            side,
            anchor: w * anchor,
            tip: w * (anchor + Vec2::new(h.x, h.y)),
            own,
        };
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

/// The round widgets of the selected corner nodes that can be rounded, in
/// document space, at `zoom`.
#[must_use]
pub fn radius_widgets(s: &Session, zoom: f64) -> Vec<(PathNodeRef, Point)> {
    let mut out = Vec::new();
    for r in selected_nodes(s) {
        let Some(n) = s.path_node(r) else {
            continue;
        };
        let Some(corner) = s
            .path_of(r.shape)
            .and_then(|p| node_corner(&p.subpaths[r.subpath], r.index))
        else {
            continue;
        };
        let w = world(s, r.shape);
        let inset = WIDGET_INSET / (zoom * scale(w));
        let local = corner.arc_middle(n.corner_radius) + corner.bisector * inset;
        out.push((r, w * local));
    }
    out
}
