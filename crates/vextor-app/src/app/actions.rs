//! What menus, shortcuts, tabs and inspector buttons ask the app to do,
//! and the Edit and Object actions on the active document's selection.

use std::path::PathBuf;

use vextor_core::{Arrange, ToolId};
use vextor_geom::Align;

use super::VextorApp;
use crate::documents::DocId;
use crate::edit_ops;
use crate::panels;
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
}

impl From<panels::PanelAction> for Action {
    /// The menu action an inspector button stands for.
    fn from(a: panels::PanelAction) -> Self {
        match a {
            panels::PanelAction::Align(how) => Self::Align(how),
            panels::PanelAction::Distribute { horizontal } => Self::Distribute { horizontal },
            panels::PanelAction::Group => Self::Group,
            panels::PanelAction::Ungroup => Self::Ungroup,
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
    /// Edit and Object actions on the active document's selection.
    pub(super) fn edit(&mut self, action: &Action) {
        if self.picker.is_some() {
            return;
        }
        let Some(s) = self.docs.session_mut() else {
            return;
        };
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
            Action::Deselect => {
                s.select(Vec::new());
                Ok(())
            }
            Action::Group => edit_ops::group(s),
            Action::Ungroup => edit_ops::ungroup(s),
            Action::Arrange(how) => edit_ops::arrange(s, *how),
            Action::Align(how) => edit_ops::align(s, *how, self.align_to_artboard),
            Action::Distribute { horizontal } => edit_ops::distribute(s, *horizontal),
            Action::Flip { horizontal } => edit_ops::flip(s, *horizontal),
            Action::Nudge { dx, dy } => edit_ops::nudge(s, f64::from(*dx), f64::from(*dy)),
            Action::Tool(tool) => {
                s.editor.tool = *tool;
                Ok(())
            }
            Action::OutlineText => match crate::outline_text::missing_font(s) {
                // Ask first: the fallback face would be outlined.
                Some(missing) => {
                    self.outline_prompt = Some(missing);
                    Ok(())
                }
                None => crate::outline_text::convert(s),
            },
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.message = Some(e);
        }
    }
}
