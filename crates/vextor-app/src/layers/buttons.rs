//! New, Duplicate and Delete layer (spec 10), the buttons over the list.

use super::{apply, current_layer};
use crate::session::Session;

/// Adds an empty layer above the current one and makes it current.
///
/// # Errors
/// A message for the user when it is refused.
pub fn new_layer(s: &mut Session) -> Result<(), String> {
    let above = current_layer(s);
    let (cmd, id) = s.doc.new_layer(above);
    apply(s, Some(cmd))?;
    s.layer = Some(id);
    s.select(Vec::new());
    Ok(())
}

/// Copies the current layer with its contents above it and makes the
/// copy current.
///
/// # Errors
/// A message for the user when it is refused.
pub fn duplicate_layer(s: &mut Session) -> Result<(), String> {
    let Some(layer) = current_layer(s) else {
        return Ok(());
    };
    let (cmd, id) = s
        .doc
        .duplicate_layer(layer)
        .map_err(|e| format!("Duplicate layer failed: {e}"))?;
    apply(s, Some(cmd))?;
    s.layer = Some(id);
    s.select(Vec::new());
    Ok(())
}

/// Deletes the current layer and what is in it (one undo step); the
/// layer below, else the new top one, becomes current.
///
/// # Errors
/// A message for the user for the last layer.
pub fn delete_layer(s: &mut Session) -> Result<(), String> {
    let Some(layer) = current_layer(s) else {
        return Ok(());
    };
    let cmd = s
        .doc
        .delete_layer(layer)
        .map_err(|e| format!("Delete layer failed: {e}"))?;
    let index = s.doc.layers.iter().position(|&l| l == layer).unwrap_or(0);
    apply(s, Some(cmd))?;
    s.layer = s.doc.layers.get(index.saturating_sub(1)).copied();
    Ok(())
}
