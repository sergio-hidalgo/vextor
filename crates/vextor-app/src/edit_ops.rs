//! Edit and Object menu operations on the selection, as functions on a
//! [`Session`] so they are tested without a window. Each edit is one
//! command, so one undo step.

use std::collections::BTreeMap;

use kurbo::{Affine, Rect as KRect, Vec2 as KVec2};
use vextor_core::{Arrange, Command, Document, NodeId, NodeKind, Parent, Rect, Transform};
use vextor_geom::{Align, retarget, union_bounds};

use crate::session::Session;

/// How far paste and duplicate move the copy, right and down, in px.
pub const PASTE_OFFSET: f64 = 10.0;

/// What the Edit and Object menus can do right now.
#[allow(clippy::struct_excessive_bools)] // one enable flag per menu item group
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Can {
    pub edit: bool,
    pub paste: bool,
    pub group: bool,
    pub ungroup: bool,
    pub distribute: bool,
}

/// Which selection operations are possible.
#[must_use]
pub fn can(s: &Session) -> Can {
    let sel = s.selected();
    Can {
        edit: !sel.is_empty(),
        paste: s.editor.clipboard.is_some(),
        group: !sel.is_empty() && common_parent(&s.doc, &sel).is_some(),
        ungroup: sel.iter().any(|&id| is_plain_group(&s.doc, id)),
        distribute: sel.len() >= 3,
    }
}

/// Copies the selection to the internal clipboard. Returns the SVG for the
/// system clipboard, or `None` when nothing visible was copied.
pub fn copy(s: &mut Session) -> Option<String> {
    let sel = s.selected();
    if sel.is_empty() {
        return None;
    }
    let payload = s.doc.copy(&sel).ok()?;
    let svg = vextor_svg::clipboard_svg(&payload);
    s.editor.clipboard = Some(payload);
    svg
}

/// Copies, then deletes the selection.
///
/// # Errors
/// A message for the user when the delete is refused.
pub fn cut(s: &mut Session) -> Result<Option<String>, String> {
    let svg = copy(s);
    delete(s)?;
    Ok(svg)
}

/// Deletes the selection.
///
/// # Errors
/// A message for the user when the delete is refused.
pub fn delete(s: &mut Session) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    apply(s, Command::Delete { ids })?;
    s.select(Vec::new());
    Ok(())
}

/// Pastes the clipboard above the selection, or at the top of the
/// topmost unlocked layer, moved by [`PASTE_OFFSET`] unless `in_place`.
///
/// # Errors
/// A message for the user when there is nowhere to paste.
pub fn paste(s: &mut Session, in_place: bool) -> Result<(), String> {
    let Some(payload) = s.editor.clipboard.clone() else {
        return Ok(());
    };
    let (parent, index) = paste_target(s).ok_or("Show or unlock a layer to paste into.")?;
    let (offset, label) = if in_place {
        (Transform::IDENTITY, "Paste in place")
    } else {
        (Transform::translate(PASTE_OFFSET, PASTE_OFFSET), "Paste")
    };
    let (cmd, roots) = s
        .doc
        .paste(&payload, parent, index, offset, label)
        .map_err(|e| format!("Could not paste: {e}"))?;
    apply(s, cmd)?;
    s.select(roots);
    Ok(())
}

/// Copies the selection next to itself, moved by [`PASTE_OFFSET`].
///
/// # Errors
/// A message for the user when the copy is refused.
pub fn duplicate(s: &mut Session) -> Result<(), String> {
    let sel = s.selected();
    let mut by_parent: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for id in sel {
        if let Some(Parent::Node(p)) = s.doc.parent_of(id) {
            by_parent.entry(p).or_default().push(id);
        }
    }
    let mut commands = Vec::new();
    let mut new = Vec::new();
    let offset = Transform::translate(PASTE_OFFSET, PASTE_OFFSET);
    for (parent, ids) in by_parent {
        let fail = |e| format!("Could not duplicate: {e}");
        let payload = s.doc.copy(&ids).map_err(fail)?;
        let index = 1 + ids
            .iter()
            .filter_map(|id| index_in(&s.doc, parent, *id))
            .max()
            .unwrap_or(0);
        let (cmd, roots) = s
            .doc
            .paste(&payload, parent, index, offset, "Duplicate")
            .map_err(fail)?;
        commands.push(cmd);
        new.extend(roots);
    }
    if commands.is_empty() {
        return Ok(());
    }
    apply(
        s,
        Command::Batch {
            label: "Duplicate",
            commands,
        },
    )?;
    s.select(new);
    Ok(())
}

/// Selects every unlocked, visible object.
pub fn select_all(s: &mut Session) {
    let all = vextor_geom::selectable(&s.doc);
    s.select(all);
}

/// Groups the selection (siblings only) and selects the group.
///
/// # Errors
/// A message for the user when the objects are not siblings.
pub fn group(s: &mut Session) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    // `Group` takes the next id for the new group.
    let id = NodeId(s.doc.next_id());
    apply(s, Command::Group { ids })?;
    s.select(vec![id]);
    Ok(())
}

/// Ungroups every selected group, selecting what was inside.
///
/// # Errors
/// A message for the user when an ungroup is refused.
pub fn ungroup(s: &mut Session) -> Result<(), String> {
    let mut commands = Vec::new();
    let mut selection = Vec::new();
    for id in s.selected() {
        if is_plain_group(&s.doc, id) {
            commands.push(Command::Ungroup { id });
            if let Some(c) = s.doc.nodes.get(&id).and_then(|n| n.container()) {
                selection.extend(c.children.iter().copied());
            }
        } else {
            selection.push(id);
        }
    }
    if commands.is_empty() {
        return Ok(());
    }
    apply(
        s,
        Command::Batch {
            label: "Ungroup",
            commands,
        },
    )?;
    s.select(selection);
    Ok(())
}

/// Restacks the selection.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn arrange(s: &mut Session, how: Arrange) -> Result<(), String> {
    match s.doc.arrange(&s.selected(), how) {
        Some(cmd) => apply(s, cmd),
        None => Ok(()),
    }
}

/// Aligns the selection: to its own bounds when two or more objects are
/// selected, to the artboard when one is or `to_artboard` is set.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn align(s: &mut Session, how: Align, to_artboard: bool) -> Result<(), String> {
    let ids = s.selected();
    let boxes = boxes(&s.doc, &ids);
    let Some(selection) = union(&boxes) else {
        return Ok(());
    };
    let target = if to_artboard || ids.len() == 1 {
        match artboard_at(&s.doc, selection.center()) {
            Some(a) => a,
            None => return Ok(()),
        }
    } else {
        selection
    };
    let moves = vextor_geom::align(&boxes.iter().map(|b| b.1).collect::<Vec<_>>(), target, how);
    move_each(s, &boxes, &moves, "Align")
}

/// Spaces three or more objects evenly, left to right when `horizontal`,
/// otherwise top to bottom.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn distribute(s: &mut Session, horizontal: bool) -> Result<(), String> {
    let boxes = boxes(&s.doc, &s.selected());
    let moves = vextor_geom::distribute(&boxes.iter().map(|b| b.1).collect::<Vec<_>>(), horizontal);
    move_each(s, &boxes, &moves, "Distribute")
}

/// Mirrors the selection about its centre.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn flip(s: &mut Session, horizontal: bool) -> Result<(), String> {
    let ids = s.selected();
    let Some(b) = union_bounds(&s.doc, &ids) else {
        return Ok(());
    };
    transform(s, vextor_geom::flip(b, horizontal), "Flip")
}

/// Moves the selection by `(dx, dy)` document px.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn nudge(s: &mut Session, dx: f64, dy: f64) -> Result<(), String> {
    transform(s, Affine::translate((dx, dy)), "Nudge")
}

/// Resizes the selection by `change` (document space, a scale), baking the
/// stretch into the shapes so strokes keep their width.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn resize(s: &mut Session, change: Affine) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    let commands = resize_commands(&s.doc, &ids, change);
    apply(
        s,
        Command::Batch {
            label: "Resize",
            commands,
        },
    )
}

/// The commands that resize `ids` in `doc` by `change`: new transforms and
/// new geometries ([`vextor_geom::resize`]).
#[must_use]
pub fn resize_commands(doc: &Document, ids: &[NodeId], change: Affine) -> Vec<Command> {
    let r = vextor_geom::resize(doc, ids, change);
    let mut commands = vec![Command::SetTransforms {
        items: r.transforms,
    }];
    if !r.geometries.is_empty() {
        commands.push(Command::SetGeometry {
            items: r.geometries,
        });
    }
    commands
}

/// Applies `change` (document space) to every selected object, as one
/// step named `label`.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn transform(s: &mut Session, change: Affine, label: &'static str) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    let items = retarget(&s.doc, &ids, change);
    apply(
        s,
        Command::Batch {
            label,
            commands: vec![Command::SetTransforms { items }],
        },
    )
}

/// Applies `cmd`, turning a refusal into a message for the user.
fn apply(s: &mut Session, cmd: Command) -> Result<(), String> {
    let label = cmd.label();
    s.apply(cmd).map_err(|e| format!("{label} failed: {e}"))
}

/// Where a paste goes: above the topmost selected object, else on top of
/// the topmost visible, unlocked layer.
fn paste_target(s: &Session) -> Option<(NodeId, usize)> {
    if let Some(&top) = s.selected().last()
        && let Some(Parent::Node(p)) = s.doc.parent_of(top)
    {
        return Some((p, index_in(&s.doc, p, top)? + 1));
    }
    s.doc
        .layers
        .iter()
        .rev()
        .filter_map(|l| s.doc.nodes.get(l))
        .find(|l| l.visible && !l.locked)
        .and_then(|l| Some((l.id, l.container()?.children.len())))
}

fn index_in(doc: &Document, parent: NodeId, id: NodeId) -> Option<usize> {
    doc.nodes
        .get(&parent)?
        .container()?
        .children
        .iter()
        .position(|&c| c == id)
}

/// The parent all `ids` share, if they do.
fn common_parent(doc: &Document, ids: &[NodeId]) -> Option<NodeId> {
    let mut parents = ids.iter().map(|&id| doc.parent_of(id));
    match parents.next()? {
        Some(Parent::Node(p)) if parents.all(|q| q == Some(Parent::Node(p))) => Some(p),
        _ => None,
    }
}

/// A group without a clip (a clipped group must be released first).
fn is_plain_group(doc: &Document, id: NodeId) -> bool {
    doc.nodes
        .get(&id)
        .is_some_and(|n| matches!(&n.kind, NodeKind::Group(c) if c.clip.is_none()))
}

fn boxes(doc: &Document, ids: &[NodeId]) -> Vec<(NodeId, KRect)> {
    ids.iter()
        .filter_map(|&id| Some((id, vextor_geom::bounds(doc, id)?)))
        .collect()
}

fn union(boxes: &[(NodeId, KRect)]) -> Option<KRect> {
    boxes.iter().map(|b| b.1).reduce(|a, b| a.union(b))
}

/// The artboard holding `p`, else the first one.
fn artboard_at(doc: &Document, p: kurbo::Point) -> Option<KRect> {
    let rect = |r: Rect| KRect::new(r.x, r.y, r.x + r.width, r.y + r.height);
    doc.artboards
        .iter()
        .map(|a| rect(a.rect))
        .find(|r| r.contains(p))
        .or_else(|| doc.artboards.first().map(|a| rect(a.rect)))
}

/// Moves each boxed object by its own vector, as one step.
fn move_each(
    s: &mut Session,
    boxes: &[(NodeId, KRect)],
    moves: &[KVec2],
    label: &'static str,
) -> Result<(), String> {
    let items: Vec<(NodeId, Transform)> = boxes
        .iter()
        .zip(moves)
        .filter(|(_, m)| m.hypot() > vextor_core::EPS)
        .flat_map(|((id, _), m)| retarget(&s.doc, &[*id], Affine::translate(*m)))
        .collect();
    if items.is_empty() {
        return Ok(());
    }
    apply(
        s,
        Command::Batch {
            label,
            commands: vec![Command::SetTransforms { items }],
        },
    )
}

#[cfg(test)]
mod tests {
    use vextor_core::{CornerRadii, Geometry, Node, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    /// A 100×100 artboard with `n` 10×10 rects at x = 0, 20, 40…
    fn session(n: u32) -> (Session, Vec<NodeId>) {
        let mut doc = new_document("a", 100.0, 100.0);
        let layer = doc.layers[0];
        let ids: Vec<NodeId> = (0..n).map(|_| doc.alloc_node_id()).collect();
        let nodes = ids
            .iter()
            .zip(0..)
            .map(|(&id, i)| {
                Node::new(
                    id,
                    NodeKind::Shape(Shape {
                        geometry: Geometry::Rect {
                            rect: Rect::new(f64::from(i) * 20.0, 0.0, 10.0, 10.0),
                            radius: CornerRadii::default(),
                        },
                        style: Style::default(),
                    }),
                )
            })
            .collect();
        if n > 0 {
            doc.apply(Command::Insert {
                parent: Some(layer),
                index: 0,
                nodes,
            })
            .expect("inserts");
        }
        (Session::new(doc), ids)
    }

    fn origin(s: &Session, id: NodeId) -> (f64, f64) {
        let b = vextor_geom::bounds(&s.doc, id).expect("visible");
        (b.x0, b.y0)
    }

    /// The step undoes to exactly the document before it.
    fn undoes(s: &mut Session, before: &Document) {
        assert!(s.undo());
        assert_eq!(&s.doc, before);
    }

    #[test]
    fn paste_offsets_and_selects_fresh_copies() {
        let (mut s, ids) = session(1);
        s.select(ids.clone());
        assert!(copy(&mut s).is_some_and(|svg| svg.starts_with("<svg")));
        let before = s.doc.clone();
        paste(&mut s, false).expect("pastes");
        let new = s.selected();
        assert_eq!(new.len(), 1);
        assert_ne!(new[0], ids[0]);
        assert_eq!(origin(&s, new[0]), (10.0, 10.0));
        undoes(&mut s, &before);
        assert_eq!(s.selected(), ids, "undo restores the selection");
    }

    #[test]
    fn paste_in_place_keeps_the_position() {
        let (mut s, ids) = session(1);
        s.select(ids);
        copy(&mut s);
        paste(&mut s, true).expect("pastes");
        assert_eq!(origin(&s, s.selected()[0]), (0.0, 0.0));
    }

    #[test]
    fn paste_into_another_session_keeps_world_position() {
        let (mut a, ids) = session(1);
        a.select(ids);
        copy(&mut a);
        let (mut b, _) = session(0);
        b.editor.clipboard = a.editor.clipboard.clone();
        paste(&mut b, true).expect("pastes");
        assert_eq!(origin(&b, b.selected()[0]), (0.0, 0.0));
    }

    #[test]
    fn cut_removes_and_paste_brings_back() {
        let (mut s, ids) = session(2);
        s.select(vec![ids[1]]);
        cut(&mut s).expect("cuts");
        assert!(!s.doc.nodes.contains_key(&ids[1]));
        assert_eq!(s.selected().len(), 0);
        paste(&mut s, true).expect("pastes");
        assert_eq!(origin(&s, s.selected()[0]), (20.0, 0.0));
    }

    #[test]
    fn duplicate_puts_fresh_copies_just_above() {
        let (mut s, ids) = session(3);
        s.select(vec![ids[0], ids[1]]);
        let before = s.doc.clone();
        duplicate(&mut s).expect("duplicates");
        let new = s.selected();
        assert_eq!(new.len(), 2);
        assert!(new.iter().all(|n| !ids.contains(n)));
        let layer = s.doc.layers[0];
        let children = &s.doc.nodes[&layer].container().expect("layer").children;
        // a, b, a', b', c: the copies sit above the topmost original.
        assert_eq!(children[2..4], new[..]);
        assert_eq!(origin(&s, new[0]), (10.0, 10.0));
        undoes(&mut s, &before);
    }

    #[test]
    fn delete_and_select_all() {
        let (mut s, ids) = session(3);
        select_all(&mut s);
        assert_eq!(s.selected(), ids);
        let before = s.doc.clone();
        delete(&mut s).expect("deletes");
        assert!(ids.iter().all(|id| !s.doc.nodes.contains_key(id)));
        undoes(&mut s, &before);
    }

    #[test]
    fn group_then_ungroup() {
        let (mut s, ids) = session(2);
        s.select(ids.clone());
        assert!(can(&s).group);
        let before = s.doc.clone();
        group(&mut s).expect("groups");
        let g = s.selected();
        assert_eq!(g.len(), 1);
        assert!(is_plain_group(&s.doc, g[0]));
        assert!(can(&s).ungroup);
        ungroup(&mut s).expect("ungroups");
        assert_eq!(s.selected(), ids);
        assert_eq!(s.doc, before);
    }

    #[test]
    fn arrange_brings_to_front() {
        let (mut s, ids) = session(3);
        s.select(vec![ids[0]]);
        let before = s.doc.clone();
        arrange(&mut s, Arrange::Front).expect("moves");
        let layer = s.doc.layers[0];
        assert_eq!(
            s.doc.nodes[&layer].container().expect("layer").children,
            vec![ids[1], ids[2], ids[0]]
        );
        undoes(&mut s, &before);
    }

    #[test]
    fn align_to_selection_and_to_artboard() {
        let (mut s, ids) = session(3);
        nudge_one(&mut s, ids[1], 0.0, 30.0);
        s.select(ids.clone());
        let before = s.doc.clone();
        align(&mut s, Align::Top, false).expect("aligns");
        assert!(ids.iter().all(|&id| origin(&s, id).1.abs() < 1e-9));
        undoes(&mut s, &before);
        // One object aligns to the artboard (100 × 100).
        s.select(vec![ids[0]]);
        align(&mut s, Align::Right, false).expect("aligns");
        assert_eq!(origin(&s, ids[0]), (90.0, 0.0));
    }

    #[test]
    fn distribute_evens_the_gaps() {
        let (mut s, ids) = session(3);
        nudge_one(&mut s, ids[1], -15.0, 0.0); // x: 0, 5, 40
        s.select(ids.clone());
        assert!(can(&s).distribute);
        distribute(&mut s, true).expect("distributes");
        assert_eq!(origin(&s, ids[1]), (20.0, 0.0));
    }

    #[test]
    fn flip_and_nudge_are_single_undo_steps() {
        let (mut s, ids) = session(2);
        s.select(ids.clone());
        let before = s.doc.clone();
        flip(&mut s, true).expect("flips");
        // Box 0..30: the rect at 0..10 lands at 20..30.
        assert_eq!(origin(&s, ids[0]), (20.0, 0.0));
        undoes(&mut s, &before);
        nudge(&mut s, 10.0, 0.0).expect("nudges");
        assert_eq!(origin(&s, ids[1]), (30.0, 0.0));
        undoes(&mut s, &before);
    }

    fn nudge_one(s: &mut Session, id: NodeId, dx: f64, dy: f64) {
        s.select(vec![id]);
        nudge(s, dx, dy).expect("nudges");
    }
}
