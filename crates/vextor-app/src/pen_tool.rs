//! The pen tool (`P`, spec 07) as a state machine over a [`Session`] fed
//! with document-space points, so it is tested without a window.
//!
//! A click adds a corner node; a click-drag adds a curved node with
//! symmetric handles (`⌥` while dragging moves only the outgoing handle,
//! making a corner). `⇧` keeps the new segment, and the dragged handle,
//! at the document's angle step. Clicking the first node closes the path;
//! `Enter` or `Esc` leaves it open; `⌫` removes the last node. Starting on
//! an open end of a path continues that path, and clicking another path's
//! open end joins the two. With no path under way, a click on a segment
//! of the selected path inserts a node there.
//!
//! The whole path is one "Draw path" undo step: each change takes the
//! tool's previous step back and records the path again, as the shape
//! tools do. If anything else changed the document in between, the path
//! under way simply ends.

use kurbo::{Point, Vec2};
use vextor_core::{NodeId, NodeType, PathNode, PathNodeRef};
use vextor_geom::{
    SegmentHit, constrain_angle, constrain_vector, handle_or_none, hit_segment, open_endpoints,
    selectable, to_kurbo,
};

use crate::path_ops;
use crate::session::Session;

mod draft;

use draft::Draft;

/// Undo label of drawing a path.
pub const LABEL: &str = "Draw path";
/// How near a node or end the pointer must be to snap to it, in screen
/// points.
pub const HIT: f64 = 6.0;
/// Movement before a press becomes a drag, in screen points.
const DRAG_THRESHOLD: f64 = 3.0;

/// What a click would do where the pointer is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hover {
    /// Over the first node of the path being drawn: close it.
    Close,
    /// Over the last node of the path being drawn: a drag reshapes its
    /// outgoing handle; a click adds nothing.
    Last,
    /// Over an open end of a path: continue it, or join it to the path
    /// being drawn.
    End(PathNodeRef),
    /// Over a segment of the selected path: insert a node there.
    Insert(NodeId, SegmentHit),
    /// Anywhere else: add a node.
    Free,
}

/// What the press under way will do on release.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    /// A node was added (or an end picked to continue): a drag shapes it.
    Node { out_only: bool },
    /// Close on release; a drag shapes the first node.
    Close,
    /// Join this end on release; a drag shapes the joining node.
    Join(PathNodeRef),
}

#[derive(Debug)]
struct Press {
    start: Point,
    pending: Pending,
    moved: bool,
    /// The document once the press had placed its node (fast drag).
    before: Box<vextor_core::Document>,
}

/// What the canvas draws for the path being drawn (document space).
#[derive(Clone, Debug, PartialEq)]
pub struct PenOverlay {
    /// The anchors of the subpath being drawn.
    pub nodes: Vec<Point>,
    /// The last node's outgoing handle tip: the rubber band leaves along it.
    pub last_out: Option<Point>,
    /// While a node is dragged out: its incoming tip, anchor, outgoing tip.
    pub handles: Option<(Option<Point>, Point, Option<Point>)>,
}

/// The pen's state between events.
#[derive(Debug, Default)]
pub struct PenTool {
    draft: Option<Draft>,
    press: Option<Press>,
}

impl PenTool {
    /// True while a path is being drawn.
    #[must_use]
    pub fn is_drawing(&self) -> bool {
        self.draft.is_some()
    }

    /// True while the button is held.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// What a click at `p` would do, at `zoom`.
    #[must_use]
    pub fn hover(&self, s: &Session, p: Point, zoom: f64) -> Hover {
        let reach = HIT / zoom;
        if let Some(d) = &self.draft {
            let first = d.nodes().first().map(|n| d.to_doc() * to_kurbo(n.anchor));
            if d.nodes().len() >= 2 && first.is_some_and(|f| (f - p).hypot() <= reach) {
                return Hover::Close;
            }
            if d.last_doc().is_some_and(|l| (l - p).hypot() <= reach) {
                return Hover::Last;
            }
        }
        if let Some(r) = self.end_at(s, p, reach) {
            return Hover::End(r);
        }
        if self.draft.is_none()
            && let Some(shape) = path_ops::edited(s)
            && let Some(path) = s.path_of(shape)
            && let Some(hit) = hit_segment(path, path_ops::world(s, shape), p, reach)
        {
            return Hover::Insert(shape, hit);
        }
        Hover::Free
    }

    /// The nearest open end of a selectable path within `reach` of `p`,
    /// other than the ends of the subpath being drawn.
    fn end_at(&self, s: &Session, p: Point, reach: f64) -> Option<PathNodeRef> {
        let own = self.draft.as_ref().map(|d| (d.shape, d.subpath));
        let mut best: Option<(PathNodeRef, f64)> = None;
        for shape in selectable(&s.doc) {
            let Some(path) = s.path_of(shape) else {
                continue;
            };
            let world = path_ops::world(s, shape);
            for (subpath, index) in open_endpoints(path) {
                if own == Some((shape, subpath)) {
                    continue;
                }
                let at = world * to_kurbo(path.subpaths[subpath].nodes[index].anchor);
                let d = (at - p).hypot();
                if d <= reach && best.is_none_or(|(_, b)| d < b) {
                    let r = PathNodeRef {
                        shape,
                        subpath,
                        index,
                    };
                    best = Some((r, d));
                }
            }
        }
        best.map(|(r, _)| r)
    }

    /// Where a click at `p` puts the next node: `p`, or with `shift` on
    /// the angle step from the last node.
    #[must_use]
    pub fn next_point(&self, s: &Session, p: Point, shift: bool) -> Point {
        match self.draft.as_ref().and_then(Draft::last_doc) {
            Some(last) if shift => constrain_angle(last, p, s.doc.grid.snap.angle_step_deg),
            _ => p,
        }
    }

    /// The path being drawn, for the canvas.
    #[must_use]
    pub fn overlay(&self) -> Option<PenOverlay> {
        let d = self.draft.as_ref()?;
        let doc = d.to_doc();
        let tip = |n: &PathNode, h: Option<vextor_core::Vec2>| {
            h.map(|h| doc * (to_kurbo(n.anchor) + Vec2::new(h.x, h.y)))
        };
        let last = d.nodes().last()?;
        let shaped = match self.press.as_ref() {
            Some(p) if p.moved => match p.pending {
                Pending::Close => d.nodes().first(),
                _ => Some(last),
            },
            _ => None,
        };
        Some(PenOverlay {
            nodes: d.nodes().iter().map(|n| doc * to_kurbo(n.anchor)).collect(),
            last_out: tip(last, last.handle_out),
            handles: shaped.map(|n| {
                (
                    tip(n, n.handle_in),
                    doc * to_kurbo(n.anchor),
                    tip(n, n.handle_out),
                )
            }),
        })
    }

    /// The pointer pressed at `p` with `shift` / `alt` as in `mods`.
    ///
    /// # Errors
    /// A message for the user when the path is refused or there is no
    /// unlocked layer to draw in.
    pub fn press(
        &mut self,
        s: &mut Session,
        p: Point,
        mods: crate::select::Mods,
        zoom: f64,
    ) -> Result<(), String> {
        self.press = None;
        self.sync(s);
        let hover = self.hover(s, p, zoom);
        let pending = match (self.draft.is_some(), hover) {
            (false, Hover::Insert(shape, hit)) => {
                return path_ops::insert_node(s, shape, hit.subpath, hit.index, hit.t);
            }
            (false, Hover::End(r)) => {
                self.continue_from(s, r);
                Pending::Node { out_only: true }
            }
            (false, _) => {
                self.start(s, p)?;
                Pending::Node { out_only: false }
            }
            (true, Hover::Close) => Pending::Close,
            (true, Hover::Last) => Pending::Node { out_only: true },
            (true, Hover::End(r)) => {
                let at = path_ops::anchor_in_doc(s, r).unwrap_or(p);
                self.add_node(s, at)?;
                Pending::Join(r)
            }
            (true, _) => {
                let at = self.next_point(s, p, mods.shift);
                self.add_node(s, at)?;
                Pending::Node { out_only: false }
            }
        };
        self.press = Some(Press {
            start: p,
            pending,
            moved: false,
            before: Box::new(s.doc.clone()),
        });
        Ok(())
    }

    /// The pointer moved to `p` with the button held: drags out handles.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn drag_to(
        &mut self,
        s: &mut Session,
        p: Point,
        mods: crate::select::Mods,
        zoom: f64,
    ) -> Result<(), String> {
        let (Some(press), Some(d)) = (&mut self.press, &mut self.draft) else {
            return Ok(());
        };
        let mut h = p - press.start;
        if !press.moved && h.hypot() * zoom < DRAG_THRESHOLD {
            return Ok(());
        }
        press.moved = true;
        if mods.shift {
            h = constrain_vector(h, s.doc.grid.snap.angle_step_deg);
        }
        let h = d.local_vec(h);
        let (index, out_only) = match press.pending {
            Pending::Close => (0, false),
            Pending::Node { out_only } => (d.nodes().len() - 1, out_only),
            Pending::Join(_) => (d.nodes().len() - 1, false),
        };
        let closing = press.pending == Pending::Close;
        let node = &mut d.nodes_mut()[index];
        // Closing drags the first node's handles the way the path runs on.
        let (forward, back) = (handle_or_none(h), handle_or_none(-h));
        if mods.alt || out_only {
            node.kind = NodeType::Corner;
            if closing {
                node.handle_in = back;
            } else {
                node.handle_out = forward;
            }
        } else {
            node.kind = NodeType::Symmetric;
            node.handle_out = forward;
            node.handle_in = back;
        }
        self.record(s, Vec::new())
    }

    /// The button was released: closes or joins when the press said so.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn release(&mut self, s: &mut Session) -> Result<(), String> {
        let Some(press) = self.press.take() else {
            return Ok(());
        };
        match press.pending {
            Pending::Node { .. } => Ok(()),
            Pending::Close => {
                if let Some(d) = &mut self.draft {
                    d.path.subpaths[d.subpath].closed = true;
                }
                let done = self.record(s, Vec::new());
                self.finish(s);
                done
            }
            Pending::Join(r) => {
                let done = self.join(s, r);
                self.finish(s);
                done
            }
        }
    }

    /// `⌫`: removes the last node drawn. Removing the only node of a new
    /// path takes the path back. Returns false when not drawing.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn backspace(&mut self, s: &mut Session) -> Result<bool, String> {
        self.sync(s);
        let Some(d) = &mut self.draft else {
            return Ok(false);
        };
        self.press = None;
        if d.nodes().len() <= d.kept {
            self.finish(s);
            return Ok(true);
        }
        d.nodes_mut().pop();
        if d.nodes().is_empty() {
            self.take_back(s);
            return Ok(true);
        }
        self.record(s, Vec::new())?;
        Ok(true)
    }

    /// Ends the path, left open (`Enter`, `Esc`, another tool…). A new
    /// path of a single node is taken back. The path stays selected.
    /// Returns true when that took a step back (so a `⌘Z` that ended the
    /// path has already done its work).
    pub fn finish(&mut self, s: &mut Session) -> bool {
        self.press = None;
        self.sync(s);
        let Some(d) = &self.draft else {
            return false;
        };
        if d.insert_at.is_some() && d.path.subpaths[d.subpath].nodes.len() < 2 {
            return self.take_back(s);
        }
        let shape = d.shape;
        self.draft = None;
        if s.doc.nodes.contains_key(&shape) {
            s.select(vec![shape]);
        }
        false
    }

    /// While a node is dragged out: the document when the press began and
    /// the path, for the canvas's fast path.
    #[must_use]
    pub fn preview(&self) -> Option<(&vextor_core::Document, NodeId)> {
        let press = self.press.as_ref().filter(|p| p.moved)?;
        Some((&press.before, self.draft.as_ref()?.shape))
    }

    /// Ends the draft when something else changed the document since the
    /// tool's last change: its step is no longer the top one.
    pub fn sync(&mut self, s: &Session) {
        if self
            .draft
            .as_ref()
            .is_some_and(|d| s.revision() != d.revision)
        {
            self.draft = None;
            self.press = None;
        }
    }
}

#[cfg(test)]
mod tests;
