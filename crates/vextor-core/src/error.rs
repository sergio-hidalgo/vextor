//! Errors of the core crate.

use thiserror::Error;

use crate::ids::{NodeId, ResourceId};
use crate::validate::Violation;

/// Why a hex colour string was rejected by [`crate::Rgba::from_hex`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ColorError {
    #[error("expected 3, 4, 6 or 8 hex digits, got {0}")]
    BadLength(usize),
    #[error("not a hex digit: {0:?}")]
    BadDigit(char),
}

/// Why a [`crate::Command`] could not be applied. The document is unchanged.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum CoreError {
    #[error("node {0:?} does not exist")]
    NodeNotFound(NodeId),
    #[error("resource {0:?} does not exist")]
    ResourceNotFound(ResourceId),
    #[error("id {0} is already in use")]
    DuplicateId(u64),
    #[error("index {index} is out of range (length {len})")]
    IndexOutOfRange { index: usize, len: usize },
    #[error("node {0:?} is not a {1}")]
    WrongKind(NodeId, &'static str),
    #[error("resource {0:?} is still used by a paint")]
    ResourceInUse(ResourceId),
    #[error("invalid command: {0}")]
    InvalidCommand(&'static str),
    #[error("the result breaks {} document invariant(s), first: {:?}", .0.len(), .0.first().map(|v| &v.0))]
    Invalid(Vec<Violation>),
}
