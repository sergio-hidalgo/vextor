//! The path the pen is drawing, and how it is recorded as one undo step:
//! starting, continuing, adding nodes, joining and taking back.

use kurbo::{Affine, Point, Vec2};
use vextor_core::{
    Command, Geometry, Node, NodeId, NodeKind, NodeType, Path, PathNode, PathNodeRef, Shape, Style,
    SubPath, Transform,
};
use vextor_geom::{from_point, join_open, to_kurbo, transform_subpath};

use super::{LABEL, PenTool};
use crate::session::Session;
use crate::{edit_ops, path_ops};

/// The style a new pen path gets: the session's default style, with its
/// stroke (1 px black when there is none) so open segments show.
#[must_use]
pub fn new_style(s: &Session) -> Style {
    let style = s.editor.default_style.clone();
    Style {
        stroke: Some(style.stroke.clone().unwrap_or_default()),
        ..style
    }
}

/// The path being drawn.
#[derive(Debug)]
pub(super) struct Draft {
    pub(super) shape: NodeId,
    pub(super) subpath: usize,
    /// The whole path, in the shape's local space.
    pub(super) path: Path,
    /// Document space → the shape's local space.
    pub(super) to_local: Affine,
    /// Nodes the subpath had before drawing (continuing a path): `⌫`
    /// never removes those.
    pub(super) kept: usize,
    /// The path as it was before drawing on it (continuing a path).
    pub(super) original: Option<Path>,
    /// Where a new shape goes (parent, index); `None` when continuing.
    pub(super) insert_at: Option<(NodeId, usize)>,
    pub(super) style: Style,
    /// The session revision after the tool's last change.
    pub(super) revision: u64,
    /// The top undo step is this path's own.
    pub(super) recorded: bool,
}

impl Draft {
    pub(super) fn nodes(&self) -> &[PathNode] {
        &self.path.subpaths[self.subpath].nodes
    }

    pub(super) fn nodes_mut(&mut self) -> &mut Vec<PathNode> {
        &mut self.path.subpaths[self.subpath].nodes
    }

    pub(super) fn to_doc(&self) -> Affine {
        self.to_local.inverse()
    }

    pub(super) fn local(&self, p: Point) -> vextor_core::Point {
        from_point(self.to_local * p)
    }

    pub(super) fn local_vec(&self, v: Vec2) -> Vec2 {
        self.to_local * (Point::ZERO + v) - self.to_local * Point::ZERO
    }

    pub(super) fn last_doc(&self) -> Option<Point> {
        self.nodes()
            .last()
            .map(|n| self.to_doc() * to_kurbo(n.anchor))
    }
}

impl PenTool {
    /// Drops the draft, taking back its step when it is still the top one.
    /// Returns true when a step was taken back.
    pub(super) fn take_back(&mut self, s: &mut Session) -> bool {
        if let Some(d) = self.draft.take()
            && d.recorded
            && s.revision() == d.revision
        {
            return s.undo();
        }
        false
    }

    /// Starts a new path with one node at `p`.
    pub(super) fn start(&mut self, s: &mut Session, p: Point) -> Result<(), String> {
        let insert_at = edit_ops::paste_target(s)
            .ok_or_else(|| "Draw failed: every layer is hidden or locked".to_owned())?;
        let node = PathNode::corner(p.x, p.y);
        self.draft = Some(Draft {
            shape: s.doc.alloc_node_id(),
            subpath: 0,
            path: Path {
                subpaths: vec![SubPath {
                    nodes: vec![node],
                    closed: false,
                }],
                fill_rule: vextor_core::FillRule::NonZero,
            },
            // The new shape's transform undoes its parent's: local space is
            // document space.
            to_local: Affine::IDENTITY,
            kept: 0,
            original: None,
            insert_at: Some(insert_at),
            style: new_style(s),
            revision: s.revision(),
            recorded: false,
        });
        self.record(s, Vec::new())
    }

    /// Continues the open subpath ending at `r`, drawing on from that end.
    pub(super) fn continue_from(&mut self, s: &mut Session, r: PathNodeRef) {
        let Some(mut path) = s.path_of(r.shape).cloned() else {
            return;
        };
        let sp = &mut path.subpaths[r.subpath];
        if r.index == 0 && sp.nodes.len() > 1 {
            sp.reverse(); // draw on from the end that was clicked
        }
        let kept = sp.nodes.len();
        let original = s.path_of(r.shape).cloned();
        let style = match &s.doc.nodes[&r.shape].kind {
            NodeKind::Shape(shape) => shape.style.clone(),
            _ => Style::default(),
        };
        self.draft = Some(Draft {
            shape: r.shape,
            subpath: r.subpath,
            path,
            to_local: path_ops::world(s, r.shape).inverse(),
            kept,
            original,
            insert_at: None,
            style,
            revision: s.revision(),
            recorded: false,
        });
        s.select(vec![r.shape]);
    }

    /// Adds a corner node at `p` (document space).
    pub(super) fn add_node(&mut self, s: &mut Session, p: Point) -> Result<(), String> {
        if let Some(d) = &mut self.draft {
            let at = d.local(p);
            d.nodes_mut().push(PathNode::corner(at.x, at.y));
        }
        self.record(s, Vec::new())
    }

    /// Joins the subpath ending at `r` onto the one drawn, whose last node
    /// was placed on `r`. Another path's subpath moves into this path.
    pub(super) fn join(&mut self, s: &mut Session, r: PathNodeRef) -> Result<(), String> {
        let Some(d) = &mut self.draft else {
            return Ok(());
        };
        let mut extra = Vec::new();
        let mut other = if r.shape == d.shape {
            d.path.subpaths[r.subpath].clone()
        } else {
            let Some(path) = s.path_of(r.shape) else {
                return Ok(());
            };
            let into = d.to_local * path_ops::world(s, r.shape);
            if path.subpaths.len() == 1 {
                extra.push(Command::Delete { ids: vec![r.shape] });
            } else {
                let mut rest = path.clone();
                rest.subpaths.remove(r.subpath);
                extra.push(Command::SetGeometry {
                    items: vec![(r.shape, Geometry::Path(rest))],
                });
            }
            transform_subpath(&path.subpaths[r.subpath], into)
        };
        if r.index != 0 {
            other.reverse(); // the joined end comes first
        }
        let mut ours = d.path.subpaths[d.subpath].clone();
        if let Some(tail) = ours.nodes.last_mut() {
            tail.kind = NodeType::Corner; // keep the handle the drag gave it
        }
        d.path.subpaths[d.subpath] = join_open(ours, other);
        if r.shape == d.shape {
            d.path.subpaths.remove(r.subpath);
            if r.subpath < d.subpath {
                d.subpath -= 1;
            }
        }
        self.record(s, extra)
    }

    /// Records the draft as the path's one undo step (with `extra`
    /// commands), replacing the tool's previous step.
    pub(super) fn record(&mut self, s: &mut Session, extra: Vec<Command>) -> Result<(), String> {
        let Some(d) = &mut self.draft else {
            return Ok(());
        };
        if d.recorded && s.revision() == d.revision {
            s.undo();
        }
        if extra.is_empty() && d.original.as_ref() == Some(&d.path) {
            // Back to the path as it was: no step at all.
            d.recorded = false;
            d.revision = s.revision();
            return Ok(());
        }
        let geometry = Geometry::Path(d.path.clone());
        let main = match d.insert_at {
            Some((parent, index)) if !s.doc.nodes.contains_key(&d.shape) => {
                let mut node = Node::new(
                    d.shape,
                    NodeKind::Shape(Shape {
                        geometry,
                        style: d.style.clone(),
                    }),
                );
                node.transform = s
                    .doc
                    .world_transform(parent)
                    .invert()
                    .unwrap_or(Transform::IDENTITY);
                Command::Insert {
                    parent: Some(parent),
                    index,
                    nodes: vec![node],
                }
            }
            _ => Command::SetGeometry {
                items: vec![(d.shape, geometry)],
            },
        };
        let mut commands = vec![main];
        commands.extend(extra);
        let result = s.apply(Command::Batch {
            label: LABEL,
            commands,
        });
        if let Err(e) = result {
            self.draft = None;
            self.press = None;
            return Err(format!("Draw failed: {e}"));
        }
        d.recorded = true;
        d.revision = s.revision();
        s.select(vec![d.shape]);
        Ok(())
    }
}
