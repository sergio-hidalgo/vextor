//! Editable Bézier paths: anchors with optional handles.

use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::math::{EPS, Point, Vec2};

/// A path shape: one or more subpaths filled with one rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    /// ≥ 1; more than one = compound path (holes).
    pub subpaths: Vec<SubPath>,
    pub fill_rule: FillRule,
}

/// One continuous run of segments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    /// ≥ 1; ≥ 2 when closed.
    pub nodes: Vec<PathNode>,
    /// Closed ⇒ a segment from the last node back to the first exists.
    pub closed: bool,
}

/// An anchor and its handles. A segment is straight when the handles on both
/// of its ends (outgoing of the start, incoming of the end) are `None`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathNode {
    pub anchor: Point,
    /// Relative to `anchor`; `None` = straight in.
    pub handle_in: Option<Vec2>,
    /// Relative to `anchor`; `None` = straight out.
    pub handle_out: Option<Vec2>,
    pub kind: NodeType,
    /// Live rounding of a corner between two straight segments; 0 = sharp.
    pub corner_radius: f64,
}

/// How the two handles of a node are tied together. The constraint only
/// applies while both handles exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// Handles independent.
    Corner,
    /// Handles collinear and opposite, lengths independent.
    Smooth,
    /// Handles opposite and of equal length.
    Symmetric,
}

/// SVG `fill-rule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// Address of one node inside a path, for selection and commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PathNodeRef {
    pub shape: NodeId,
    pub subpath: usize,
    pub index: usize,
}

/// Which handle of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    In,
    Out,
}

impl PathNode {
    /// A sharp corner node with no handles.
    #[must_use]
    pub const fn corner(x: f64, y: f64) -> Self {
        Self {
            anchor: Point::new(x, y),
            handle_in: None,
            handle_out: None,
            kind: NodeType::Corner,
            corner_radius: 0.0,
        }
    }

    /// The same node seen when the path runs the other way.
    #[must_use]
    pub fn reversed(self) -> Self {
        Self {
            handle_in: self.handle_out,
            handle_out: self.handle_in,
            ..self
        }
    }

    /// Restores the [`NodeType`] constraint after `keep` was edited: the
    /// other handle is turned (and, for `Symmetric`, resized) to match it.
    pub fn enforce(&mut self, keep: Side) {
        let (Some(h_in), Some(h_out)) = (self.handle_in, self.handle_out) else {
            return;
        };
        let (kept, other) = match keep {
            Side::In => (h_in, h_out),
            Side::Out => (h_out, h_in),
        };
        let fixed = match self.kind {
            NodeType::Corner => return,
            NodeType::Symmetric => -kept,
            NodeType::Smooth => match kept.normalized() {
                Some(dir) => dir.scale(-other.length()),
                None => return,
            },
        };
        match keep {
            Side::In => self.handle_out = Some(fixed),
            Side::Out => self.handle_in = Some(fixed),
        }
    }

    /// Restores the [`NodeType`] constraint without favouring either handle:
    /// both are aligned to their average direction; `Symmetric` also
    /// averages their lengths.
    pub fn enforce_balanced(&mut self) {
        let (Some(h_in), Some(h_out)) = (self.handle_in, self.handle_out) else {
            return;
        };
        let Some(dir) = (h_out + -h_in).normalized() else {
            return;
        };
        let (len_in, len_out) = match self.kind {
            NodeType::Corner => return,
            NodeType::Smooth => (h_in.length(), h_out.length()),
            NodeType::Symmetric => {
                let avg = f64::midpoint(h_in.length(), h_out.length());
                (avg, avg)
            }
        };
        self.handle_in = Some(dir.scale(-len_in));
        self.handle_out = Some(dir.scale(len_out));
    }

    /// True when the handles satisfy this node's [`NodeType`] within [`EPS`].
    #[must_use]
    pub fn satisfies_type(&self) -> bool {
        let (Some(h_in), Some(h_out)) = (self.handle_in, self.handle_out) else {
            return true;
        };
        match self.kind {
            NodeType::Corner => true,
            NodeType::Symmetric => (h_in + h_out).length() <= EPS * (1.0 + h_out.length()),
            NodeType::Smooth => match (h_in.normalized(), h_out.normalized()) {
                (Some(a), Some(b)) => a.cross(b).abs() <= EPS && a.dot(b) < 0.0,
                _ => true,
            },
        }
    }

    /// True when anchor, handles and radius are finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.anchor.is_finite()
            && self.handle_in.is_none_or(Vec2::is_finite)
            && self.handle_out.is_none_or(Vec2::is_finite)
            && self.corner_radius.is_finite()
    }
}

impl SubPath {
    /// Number of segments: `n - 1` when open, `n` when closed.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        if self.closed {
            self.nodes.len()
        } else {
            self.nodes.len().saturating_sub(1)
        }
    }

    /// Runs the subpath the other way.
    pub fn reverse(&mut self) {
        self.nodes.reverse();
        for n in &mut self.nodes {
            *n = n.reversed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_handles(kind: NodeType, h_in: Vec2, h_out: Vec2) -> PathNode {
        PathNode {
            handle_in: Some(h_in),
            handle_out: Some(h_out),
            kind,
            ..PathNode::corner(0.0, 0.0)
        }
    }

    #[test]
    fn symmetric_mirrors_the_kept_handle() {
        let mut n = with_handles(
            NodeType::Symmetric,
            Vec2::new(-1.0, 0.0),
            Vec2::new(3.0, 4.0),
        );
        n.enforce(Side::Out);
        assert_eq!(n.handle_in, Some(Vec2::new(-3.0, -4.0)));
        assert!(n.satisfies_type());
    }

    #[test]
    fn smooth_keeps_the_other_length() {
        let mut n = with_handles(NodeType::Smooth, Vec2::new(-2.0, 0.0), Vec2::new(0.0, 5.0));
        n.enforce(Side::Out);
        assert_eq!(n.handle_in, Some(Vec2::new(0.0, -2.0)));
        assert!(n.satisfies_type());
    }

    #[test]
    fn balanced_averages_direction_and_length() {
        let mut n = with_handles(
            NodeType::Symmetric,
            Vec2::new(0.0, 2.0),
            Vec2::new(4.0, 0.0),
        );
        n.enforce_balanced();
        assert!(n.satisfies_type());
        let out = n.handle_out.expect("kept");
        assert!((out.length() - 3.0).abs() < EPS);
    }

    #[test]
    fn corner_is_never_constrained() {
        let mut n = with_handles(NodeType::Corner, Vec2::new(0.0, 2.0), Vec2::new(4.0, 0.0));
        let before = n;
        n.enforce(Side::In);
        n.enforce_balanced();
        assert_eq!(n, before);
        assert!(n.satisfies_type());
    }
}
