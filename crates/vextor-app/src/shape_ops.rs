//! Path operations (spec 11) on the selection: Union, Subtract,
//! Intersect, Exclude, Divide, Offset Path, Outline Stroke, Simplify and
//! Make / Release Compound Path. One table ([`ShapeOp`]), so the tools
//! strip, Object ▸ Path Operations and the shortcuts agree on what is
//! possible and why not. Each is one undo step.
//!
//! Text takes part in booleans only as outlines: the app asks first
//! ([`ShapeOp::needs_outlines`]), then the text's outlines are combined
//! directly, in the same step.

mod combine;
mod compound;
mod modify;
mod operand;
#[cfg(test)]
mod tests;

use vextor_core::Selection;
use vextor_geom::BooleanOp;

#[cfg(test)]
pub use compound::FILL_RULE;
pub use compound::set_fill_rule;
pub use modify::{OffsetSettings, offset_selection, simplify_selection};

use crate::session::Session;

/// Undo label of Offset Path.
pub const OFFSET: &str = "Offset Path";
/// Undo label of Simplify.
pub const SIMPLIFY: &str = "Simplify";
/// Undo label of Outline Stroke.
pub const OUTLINE: &str = "Outline Stroke";
/// Undo label of Make Compound Path.
pub const MAKE_COMPOUND: &str = "Make Compound Path";
/// Undo label of Release Compound Path.
pub const RELEASE_COMPOUND: &str = "Release Compound Path";

/// A path operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
    Divide,
    Offset,
    OutlineStroke,
    Simplify,
    MakeCompound,
    ReleaseCompound,
}

impl ShapeOp {
    /// The operations that combine two or more objects.
    pub const COMBINE: [Self; 5] = [
        Self::Union,
        Self::Subtract,
        Self::Intersect,
        Self::Exclude,
        Self::Divide,
    ];
    /// The operations on each selected shape.
    pub const MODIFY: [Self; 3] = [Self::Offset, Self::OutlineStroke, Self::Simplify];
    /// Compound paths.
    pub const COMPOUND: [Self; 2] = [Self::MakeCompound, Self::ReleaseCompound];

    /// Its menu text (… when it asks for settings first).
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Union => "Union",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
            Self::Exclude => "Exclude",
            Self::Divide => "Divide",
            Self::Offset => "Offset Path…",
            Self::OutlineStroke => OUTLINE,
            Self::Simplify => "Simplify…",
            Self::MakeCompound => MAKE_COMPOUND,
            Self::ReleaseCompound => RELEASE_COMPOUND,
        }
    }

    /// What it needs, shown while it is disabled.
    #[must_use]
    pub fn why(self) -> &'static str {
        match self {
            Self::Union | Self::Subtract | Self::Intersect | Self::Exclude | Self::Divide => {
                "Select two or more shapes, texts or groups"
            }
            Self::Offset => "Select a shape",
            Self::OutlineStroke => "Select a shape with a stroke",
            Self::Simplify => "Select a path (Convert to Path first)",
            Self::MakeCompound => "Select two or more shapes",
            Self::ReleaseCompound => "Select a path with several parts",
        }
    }

    /// The boolean it stands for, if it is one.
    fn boolean(self) -> Option<BooleanOp> {
        Some(match self {
            Self::Union => BooleanOp::Union,
            Self::Subtract => BooleanOp::Subtract,
            Self::Intersect => BooleanOp::Intersect,
            Self::Exclude => BooleanOp::Exclude,
            _ => return None,
        })
    }

    /// True when it would do something now.
    #[must_use]
    pub fn can(self, s: &Session) -> bool {
        let objects = matches!(s.editor.selection, Selection::Objects(_));
        let ids = s.selected();
        let shapes = modify::shapes(s);
        let all_shapes = !shapes.is_empty() && shapes.len() == ids.len();
        match self {
            Self::Union | Self::Subtract | Self::Intersect | Self::Exclude | Self::Divide => {
                objects && ids.len() >= 2 && ids.iter().all(|&id| operand::eligible(&s.doc, id))
            }
            Self::Offset => all_shapes,
            Self::OutlineStroke => all_shapes && shapes.iter().any(|(_, sh)| modify::stroked(sh)),
            Self::Simplify => {
                all_shapes
                    && shapes
                        .iter()
                        .all(|(_, sh)| matches!(sh.geometry, vextor_core::Geometry::Path(_)))
            }
            Self::MakeCompound => objects && all_shapes && shapes.len() >= 2,
            Self::ReleaseCompound => {
                all_shapes && shapes.iter().any(|(_, sh)| compound::is_compound(sh))
            }
        }
    }

    /// True when it asks for settings in a dialog (with a live preview)
    /// instead of running at once.
    #[must_use]
    pub fn has_dialog(self) -> bool {
        matches!(self, Self::Offset | Self::Simplify)
    }

    /// True when the selection holds text, which this operation takes as
    /// outlines: the app asks "Convert text to outlines first?".
    #[must_use]
    pub fn needs_outlines(self, s: &Session) -> bool {
        (self.boolean().is_some() || self == Self::Divide)
            && s.selected().iter().any(|&id| operand::has_text(&s.doc, id))
    }

    /// Does it, as one undo step (Offset and Simplify with their default
    /// settings; the app runs them through their dialog).
    ///
    /// # Errors
    /// A message for the user when it cannot be done.
    pub fn run(self, s: &mut Session) -> Result<(), String> {
        if !self.can(s) {
            return Err(format!(
                "{}: {}",
                self.title().trim_end_matches('…'),
                self.why()
            ));
        }
        match self {
            Self::Union | Self::Subtract | Self::Intersect | Self::Exclude => {
                let op = self.boolean().expect("a boolean: matched above");
                combine::boolean(s, op, self.title())
            }
            Self::Divide => combine::divide_selection(s, self.title()),
            Self::Offset => offset_selection(s, OffsetSettings::default()),
            Self::OutlineStroke => modify::outline_selection(s),
            Self::Simplify => simplify_selection(s, DEFAULT_TOLERANCE).map(|_| ()),
            Self::MakeCompound => compound::make(s),
            Self::ReleaseCompound => compound::release(s),
        }
    }
}

/// Simplify's starting tolerance, in px.
pub const DEFAULT_TOLERANCE: f64 = 0.5;
