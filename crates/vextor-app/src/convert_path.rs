//! Object ▸ Convert to Path (`⇧⌘P`, spec 06): each selected parametric
//! shape becomes an editable path that draws exactly the same, in one
//! undo step. Paths and other kinds are left alone.

use vextor_core::{Command, Geometry, NodeId, NodeKind};
use vextor_geom::geometry_to_path;

use crate::session::Session;

/// Undo label of the conversion.
pub const LABEL: &str = "Convert to Path";

/// The selected shapes that are not paths yet.
#[must_use]
pub fn convertible(s: &Session) -> Vec<NodeId> {
    s.selected()
        .into_iter()
        .filter(|id| {
            matches!(
                s.doc.nodes.get(id).map(|n| &n.kind),
                Some(NodeKind::Shape(shape)) if !matches!(shape.geometry, Geometry::Path(_))
            )
        })
        .collect()
}

/// Converts every [`convertible`] shape.
///
/// # Errors
/// A message for the user when nothing can be converted or the change is
/// refused.
pub fn convert(s: &mut Session) -> Result<(), String> {
    let items: Vec<_> = convertible(s)
        .into_iter()
        .filter_map(|id| match &s.doc.nodes.get(&id)?.kind {
            NodeKind::Shape(shape) => Some((id, geometry_to_path(&shape.geometry))),
            _ => None,
        })
        .collect();
    if items.is_empty() {
        return Err("Convert to Path: select a rectangle, ellipse, polygon, star or line".into());
    }
    s.apply(Command::Batch {
        label: LABEL,
        commands: vec![Command::ConvertToPath { items }],
    })
    .map_err(|e| format!("Convert to Path failed: {e}"))
}

#[cfg(test)]
mod tests {
    use kurbo::Point;
    use vextor_core::ToolId;
    use vextor_geom::DrawMods;

    use super::*;
    use crate::new_doc::new_document;
    use crate::shape_tool::{Release, ShapeTool};

    fn drawn(s: &mut Session, tool: ToolId) -> NodeId {
        let mut t = ShapeTool::default();
        t.press(s, tool, Point::new(10.0, 10.0)).expect("presses");
        t.drag_to(s, Point::new(60.0, 40.0), DrawMods::default())
            .expect("drags");
        match t.release(s) {
            Release::Drawn(id) => id,
            other => panic!("drawn, got {other:?}"),
        }
    }

    fn is_path(s: &Session, id: NodeId) -> bool {
        matches!(&s.doc.nodes[&id].kind, NodeKind::Shape(shape) if matches!(shape.geometry, Geometry::Path(_)))
    }

    #[test]
    fn selected_shapes_become_paths_in_one_step() {
        let mut s = Session::new(new_document("a", 200.0, 200.0));
        let a = drawn(&mut s, ToolId::Star);
        let b = drawn(&mut s, ToolId::Ellipse);
        s.select(vec![a, b]);
        assert_eq!(convertible(&s).len(), 2);
        convert(&mut s).expect("converts");
        assert!(is_path(&s, a) && is_path(&s, b));
        assert_eq!(s.edit_menu().undo, Some(LABEL));
        assert!(convertible(&s).is_empty(), "paths are done");
        s.undo();
        assert!(!is_path(&s, a) && !is_path(&s, b));
    }

    #[test]
    fn nothing_to_convert_is_refused_without_a_step() {
        let mut s = Session::new(new_document("a", 200.0, 200.0));
        assert!(convert(&mut s).is_err());
        assert_eq!(s.edit_menu().undo, None);
    }
}
