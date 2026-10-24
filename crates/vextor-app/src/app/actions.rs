//! What menus, shortcuts, tabs and inspector buttons ask the app to do,
//! and the Edit and Object actions on the active document's selection.

use std::path::PathBuf;

use vextor_core::{Arrange, ToolId};
use vextor_geom::Align;

use super::VextorApp;
use crate::documents::DocId;
use crate::edit_ops;
use crate::panels;
use crate::path_dialog::PathDialog;
use crate::path_ops::{self, PathAction};
use crate::session::Session;
use crate::shape_ops::ShapeOp;
use crate::snap_settings::SnapOption;
use crate::tabs::TabAction;

/// A user request from a menu, a shortcut, a tab or the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    New,
    Open,
    OpenRecent(PathBuf),
    ClearRecent,
    Save,
    SaveAs,
    /// File ▸ Close: the active document.
    Close,
    /// A tab's close button.
    CloseTab(DocId),
    Activate(DocId),
    /// Rename a document from its tab (the typed text, not yet cleaned).
    RenameDocument(DocId, String),
    CycleTab {
        forward: bool,
    },
    Undo,
    Redo,
    Fit,
    ActualSize,
    /// View ▸ Zoom In / Zoom Out (spec 08b): one stop.
    Zoom {
        inward: bool,
    },
    /// View ▸ Show Grid (spec 08).
    ToggleGrid,
    /// A View ▸ snapping toggle (spec 08).
    Snap(SnapOption),
    /// Object ▸ Add Artboard… (spec 08): the size picker, adding.
    AddArtboard,
    Cut,
    Copy,
    Paste,
    PasteInPlace,
    Duplicate,
    Delete,
    SelectAll,
    Deselect,
    Group,
    Ungroup,
    /// Object ▸ Make Clip / Release Clip (spec 10).
    MakeClip,
    ReleaseClip,
    Arrange(Arrange),
    Align(Align),
    Distribute {
        horizontal: bool,
    },
    Flip {
        horizontal: bool,
    },
    Nudge {
        dx: i32,
        dy: i32,
    },
    Tool(ToolId),
    OutlineText,
    ConvertToPath,
    /// Object ▸ Make Pattern (spec 09).
    MakePattern,
    /// A path command (spec 07).
    Path(PathAction),
    /// A path operation (spec 11).
    Shape(ShapeOp),
}

impl From<panels::PanelAction> for Action {
    /// The menu action an inspector button stands for.
    fn from(a: panels::PanelAction) -> Self {
        match a {
            panels::PanelAction::Align(how) => Self::Align(how),
            panels::PanelAction::Distribute { horizontal } => Self::Distribute { horizontal },
            panels::PanelAction::Arrange(how) => Self::Arrange(how),
            panels::PanelAction::Group => Self::Group,
            panels::PanelAction::Ungroup => Self::Ungroup,
            panels::PanelAction::AddArtboard => Self::AddArtboard,
            panels::PanelAction::Flip { horizontal } => Self::Flip { horizontal },
            panels::PanelAction::Shape(op) => Self::Shape(op),
        }
    }
}

impl From<TabAction> for Action {
    fn from(a: TabAction) -> Self {
        match a {
            TabAction::Activate(id) => Self::Activate(id),
            TabAction::Close(id) => Self::CloseTab(id),
            TabAction::New => Self::New,
            TabAction::Rename(id, name) => Self::RenameDocument(id, name),
        }
    }
}

impl VextorApp {
    /// While the pen draws (spec 07), ⌫ removes its last node and Esc ends
    /// the path; anything else ends the path first (⌘Z then undoes it).
    /// Returns true when `action` was used up.
    pub(super) fn pen_action(&mut self, action: &Action) -> bool {
        if !self.canvas.pen_drawing() {
            return false;
        }
        let Some(s) = self.docs.session_mut() else {
            return false;
        };
        match action {
            Action::Delete => {
                self.message = self.canvas.pen_backspace(s).or(self.message.take());
                true
            }
            Action::Deselect => {
                let _ = self.canvas.finish_pen(s);
                true
            }
            Action::Fit
            | Action::ActualSize
            | Action::Zoom { .. }
            | Action::ToggleGrid
            | Action::Snap(_) => false,
            // ⌘Z that took back a lone first node has done its undo.
            Action::Undo => self.canvas.finish_pen(s),
            _ => {
                let _ = self.canvas.finish_pen(s);
                false
            }
        }
    }

    /// Edit and Object actions on the active document's selection.
    pub(super) fn edit(&mut self, action: &Action) {
        if self.picker.is_some() {
            return;
        }
        let Some(s) = self.docs.session_mut() else {
            return;
        };
        if let Some(result) = node_edit(s, action, self.align_to_artboard) {
            if let Err(e) = result {
                self.message = Some(e);
            }
            return;
        }
        let result = match action {
            Action::Cut => edit_ops::cut(s).map(|svg| self.system_clipboard = svg),
            Action::Copy => {
                self.system_clipboard = edit_ops::copy(s);
                Ok(())
            }
            Action::Paste => edit_ops::paste(s, false),
            Action::PasteInPlace => edit_ops::paste(s, true),
            Action::Duplicate => edit_ops::duplicate(s),
            Action::Delete => edit_ops::delete(s),
            Action::SelectAll => {
                edit_ops::select_all(s);
                Ok(())
            }
            // Esc leaves the zoom tool (spec 08b), keeping the selection.
            Action::Deselect if s.editor.tool == ToolId::Zoom => {
                s.editor.tool = ToolId::Select;
                Ok(())
            }
            Action::Deselect => {
                s.select(Vec::new());
                Ok(())
            }
            Action::Group => edit_ops::group(s),
            Action::Ungroup => edit_ops::ungroup(s),
            Action::MakeClip => edit_ops::make_clip(s),
            Action::ReleaseClip => edit_ops::release_clip(s),
            Action::Arrange(how) => edit_ops::arrange(s, *how),
            Action::Align(how) => edit_ops::align(s, *how, self.align_to_artboard),
            Action::Distribute { horizontal } => edit_ops::distribute(s, *horizontal),
            Action::Flip { horizontal } => edit_ops::flip(s, *horizontal),
            Action::Nudge { dx, dy } => edit_ops::nudge(s, f64::from(*dx), f64::from(*dy)),
            Action::Tool(tool) => {
                // Leaving node editing keeps the path, not its nodes.
                if *tool != ToolId::Node && path_ops::editing_nodes(s) {
                    s.select(s.selected());
                }
                s.editor.tool = *tool;
                Ok(())
            }
            Action::Path(a) => a.run(s),
            Action::OutlineText => match crate::outline_text::missing_font(s) {
                // Ask first: the fallback face would be outlined.
                Some(missing) => {
                    self.outline_prompt = Some(missing);
                    Ok(())
                }
                None => crate::outline_text::convert(s),
            },
            Action::ConvertToPath => crate::convert_path::convert(s),
            // Offset and Simplify ask for settings, previewed live; text
            // takes part only once the user agrees to outline it.
            Action::Shape(op) if op.can(s) && op.has_dialog() => {
                self.path_dialog = PathDialog::new(*op);
                Ok(())
            }
            Action::Shape(op) if op.can(s) && op.needs_outlines(s) => {
                self.shape_prompt = Some(*op);
                Ok(())
            }
            Action::Shape(op) => op.run(s),
            Action::MakePattern => crate::paint_edit::make_pattern(s),
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.message = Some(e);
        }
    }
}

/// In node editing (spec 07) — path nodes or a segment selected, or the
/// node tool on — Delete, the arrows, Align, Distribute, Esc and Select
/// All act on nodes (`to_artboard`: Align to Artboard): the result, or
/// `None` when `action` is left to the objects.
fn node_edit(s: &mut Session, action: &Action, to_artboard: bool) -> Option<Result<(), String>> {
    let nodes = path_ops::editing_nodes(s);
    if !nodes && s.editor.tool != ToolId::Node {
        return None;
    }
    match action {
        // With a segment selected these do nothing: never the whole shape.
        Action::Delete if nodes => Some(path_ops::delete_nodes(s)),
        Action::Nudge { dx, dy } if nodes => {
            Some(path_ops::nudge_nodes(s, f64::from(*dx), f64::from(*dy)))
        }
        Action::Align(how) if nodes => Some(path_ops::align_nodes(s, *how, to_artboard)),
        Action::Distribute { horizontal } if nodes => {
            Some(path_ops::distribute_nodes(s, *horizontal))
        }
        // Esc steps out: nodes → the path → the selection tool.
        Action::Deselect => {
            match path_ops::edited(s) {
                Some(shape) if nodes => path_ops::select_nodes(s, shape, Vec::new()),
                _ => s.editor.tool = ToolId::Select,
            }
            Some(Ok(()))
        }
        Action::SelectAll if path_ops::edited(s).is_some() => {
            path_ops::select_all_nodes(s);
            Some(Ok(()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{
        Command, Geometry, Node, NodeKind, Path, PathNode, Selection, Shape, SubPath,
    };

    use super::*;
    use crate::new_doc::new_document;

    /// A session with one closed square path, edited with the node tool.
    fn session() -> (Session, vextor_core::NodeId) {
        let mut doc = new_document("a", 200.0, 200.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let nodes = [(0.0, 0.0), (50.0, 0.0), (50.0, 50.0), (0.0, 50.0)]
            .map(|(x, y)| PathNode::corner(x, y))
            .to_vec();
        let shape = Shape {
            geometry: Geometry::Path(Path {
                subpaths: vec![SubPath {
                    nodes,
                    closed: true,
                }],
                fill_rule: vextor_core::FillRule::NonZero,
            }),
            style: vextor_core::Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(shape))],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        s.editor.tool = ToolId::Node;
        (s, id)
    }

    #[test]
    fn delete_with_a_segment_selected_never_deletes_the_shape() {
        let (mut s, id) = session();
        s.editor.selection = Selection::Segment {
            shape: id,
            subpath: 0,
            index: 0,
        };
        assert_eq!(node_edit(&mut s, &Action::Delete, false), Some(Ok(())));
        assert!(s.doc.nodes.contains_key(&id));
        assert_eq!(s.edit_menu().undo, None);
    }

    #[test]
    fn delete_and_arrows_act_on_selected_nodes() {
        let (mut s, id) = session();
        let r = vextor_core::PathNodeRef {
            shape: id,
            subpath: 0,
            index: 1,
        };
        path_ops::select_nodes(&mut s, id, vec![r]);
        node_edit(&mut s, &Action::Nudge { dx: 0, dy: 10 }, false)
            .expect("handled")
            .expect("moves");
        assert_eq!(
            s.path_node(r).expect("node").anchor,
            vextor_core::Point::new(50.0, 10.0)
        );
        node_edit(&mut s, &Action::Delete, false)
            .expect("handled")
            .expect("deletes");
        assert_eq!(s.path_of(id).expect("path").subpaths[0].nodes.len(), 3);
    }

    #[test]
    fn escape_steps_out_from_nodes_to_the_path_to_the_selection_tool() {
        let (mut s, id) = session();
        path_ops::select_all_nodes(&mut s);
        assert_eq!(path_ops::selected_nodes(&s).len(), 4, "⌘A took every node");
        node_edit(&mut s, &Action::Deselect, false);
        assert_eq!(s.editor.selection, Selection::Objects(vec![id]));
        assert_eq!(s.editor.tool, ToolId::Node);
        node_edit(&mut s, &Action::Deselect, false);
        assert_eq!(s.editor.tool, ToolId::Select);
        assert_eq!(s.selected(), vec![id], "the path stays selected");
        assert_eq!(
            node_edit(&mut s, &Action::Deselect, false),
            None,
            "then Esc deselects as usual"
        );
    }

    #[test]
    fn align_and_distribute_act_on_selected_nodes_not_the_shape() {
        let (mut s, id) = session();
        let at = |index| vextor_core::PathNodeRef {
            shape: id,
            subpath: 0,
            index,
        };
        path_ops::select_nodes(&mut s, id, vec![at(0), at(2)]);
        let transform = s.doc.nodes[&id].transform;
        node_edit(&mut s, &Action::Align(vextor_geom::Align::Left), false)
            .expect("handled")
            .expect("aligns");
        assert_eq!(s.doc.nodes[&id].transform, transform, "the shape stays");
        assert_eq!(s.path_node(at(2)).expect("node").anchor.x, 0.0);
        assert_eq!(s.edit_menu().undo, Some("Align nodes"));
        assert_eq!(
            node_edit(&mut s, &Action::Distribute { horizontal: true }, false),
            Some(Ok(())),
            "two nodes: nothing to space, still not the shape"
        );
    }
}
