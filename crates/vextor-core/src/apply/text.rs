//! Text commands: content, font and metrics, frame.

use super::Tx;
use crate::command::Command;
use crate::error::CoreError;
use crate::ids::NodeId;
use crate::node::NodeKind;
use crate::text::Text;

/// Runs [`Command::SetText`], [`Command::SetTextStyle`] or
/// [`Command::SetTextFrame`].
pub(super) fn run(tx: &mut Tx<'_>, cmd: Command) -> Result<(), CoreError> {
    match cmd {
        Command::SetText { id, content } => text_mut(tx, id)?.content = content,
        Command::SetTextStyle {
            ids,
            font,
            size,
            line_height,
            letter_spacing,
            align,
        } => {
            for id in ids {
                let t = text_mut(tx, id)?;
                t.font.clone_from(&font);
                t.size = size;
                t.line_height = line_height;
                t.letter_spacing = letter_spacing;
                t.align = align;
            }
        }
        Command::SetTextFrame { ids, frame } => {
            for id in ids {
                text_mut(tx, id)?.frame.clone_from(&frame);
            }
        }
        _ => return Err(CoreError::InvalidCommand("not a text command")),
    }
    Ok(())
}

/// Write access to a text object.
fn text_mut<'t>(tx: &'t mut Tx<'_>, id: NodeId) -> Result<&'t mut Text, CoreError> {
    match &mut tx.node_mut(id)?.kind {
        NodeKind::Text(t) => Ok(t),
        _ => Err(CoreError::WrongKind(id, "text")),
    }
}
