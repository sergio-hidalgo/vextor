//! Object ▸ Make Clip and Release Clip (spec 10): the topmost selected
//! shape clips the objects under it, inside a new group.

use vextor_core::{Command, NodeId, NodeKind};

use super::{apply, common_parent};
use crate::session::Session;

/// What Make Clip needs, for its disabled tooltip.
pub const MAKE_CLIP_HINT: &str = "Select objects in one group or layer, a shape on top";

/// Why Make Clip is disabled, or `None` when it can run.
#[must_use]
pub fn make_clip_why(s: &Session) -> Option<&'static str> {
    let sel = s.selected();
    if sel.len() < 2 {
        return Some("Select a shape and the objects under it");
    }
    if common_parent(&s.doc, &sel).is_none() {
        return Some("Select objects in the same group or layer");
    }
    let top = sel.last().and_then(|id| s.doc.nodes.get(id));
    if !top.is_some_and(|n| matches!(n.kind, NodeKind::Shape(_))) {
        return Some("The topmost selected object must be a shape");
    }
    None
}

/// The selected groups that clip.
fn clipped_groups(s: &Session) -> Vec<NodeId> {
    s.selected()
        .into_iter()
        .filter(|id| {
            s.doc
                .nodes
                .get(id)
                .is_some_and(|n| matches!(&n.kind, NodeKind::Group(c) if c.clip.is_some()))
        })
        .collect()
}

/// Whether Release Clip has something to release.
#[must_use]
pub fn can_release_clip(s: &Session) -> bool {
    !clipped_groups(s).is_empty()
}

/// Groups the selection, clipped by its topmost shape, and selects the
/// group.
///
/// # Errors
/// A message for the user when it is refused.
pub fn make_clip(s: &mut Session) -> Result<(), String> {
    if let Some(why) = make_clip_why(s) {
        return Err(format!("Make clip failed: {why}"));
    }
    // `MakeClip` takes the next id for the new group, like `Group`.
    let id = NodeId(s.doc.next_id());
    apply(s, Command::MakeClip { ids: s.selected() })?;
    s.select(vec![id]);
    Ok(())
}

/// Stops every selected clip group clipping; their clip shapes show again
/// and the groups stay selected.
///
/// # Errors
/// A message for the user when it is refused.
pub fn release_clip(s: &mut Session) -> Result<(), String> {
    let commands: Vec<Command> = clipped_groups(s)
        .into_iter()
        .map(|id| Command::ReleaseClip { id })
        .collect();
    if commands.is_empty() {
        return Ok(());
    }
    apply(
        s,
        Command::Batch {
            label: "Release clip",
            commands,
        },
    )
}
