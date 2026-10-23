//! Naming and deleting a gradient or pattern (spec 09).

use vextor_core::{Command, Document, NodeId, Paint, Resource, ResourceId};

use super::{Edit, Target, edit_commands, paint};

/// Longest gradient or pattern name, in characters.
pub const MAX_RESOURCE_NAME_CHARS: usize = 64;

/// One undo step naming gradient or pattern `id` (trimmed, at most
/// [`MAX_RESOURCE_NAME_CHARS`]; blank removes the name, so menus show
/// "Pattern 2" again). `None` when nothing would change.
#[must_use]
pub fn rename_resource(doc: &Document, id: ResourceId, name: &str) -> Option<Command> {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_RESOURCE_NAME_CHARS)
        .collect();
    let name = (!name.is_empty()).then_some(name);
    let mut resource = doc.resources.get(&id)?.clone();
    let label = match &mut resource {
        Resource::Gradient(g) if g.name != name => {
            g.name = name;
            "Rename gradient"
        }
        Resource::Pattern(p) if p.name != name => {
            p.name = name;
            "Rename pattern"
        }
        _ => return None,
    };
    Some(Command::Batch {
        label,
        commands: vec![Command::PutResource {
            id,
            resource,
            nodes: Vec::new(),
        }],
    })
}

/// One undo step deleting gradient or pattern `id`: every fill, stroke,
/// text background and border painted with it gets no paint (a stroke or
/// border goes), then the resource and its pattern content go. `None` for
/// a missing resource.
#[must_use]
pub fn delete_resource(doc: &Document, id: ResourceId) -> Option<Command> {
    let label = match doc.resources.get(&id)? {
        Resource::Gradient(_) => "Delete gradient",
        Resource::Pattern(_) => "Delete pattern",
    };
    let uses =
        |p: Option<Paint>| matches!(p, Some(Paint::Gradient(r) | Paint::Pattern(r)) if r == id);
    // Each repaint is worked out on the document as the ones before it
    // left it, so a fill and a stroke on one object both change.
    let mut scratch = doc.clone();
    let mut commands = Vec::new();
    let ids: Vec<NodeId> = doc.nodes.keys().copied().collect();
    for node_id in ids {
        for target in [
            Target::Fill,
            Target::Stroke,
            Target::Background,
            Target::Border,
        ] {
            let painted = scratch
                .nodes
                .get(&node_id)
                .is_some_and(|n| uses(paint(n, target)));
            if !painted {
                continue;
            }
            for cmd in edit_commands(&scratch, &[node_id], target, &Edit::Paint(Paint::None)) {
                scratch.apply(cmd.clone()).ok()?;
                commands.push(cmd);
            }
        }
    }
    commands.push(Command::RemoveResource { id });
    Some(Command::Batch { label, commands })
}
