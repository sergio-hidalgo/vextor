//! Shared images (format version 4, spec 14): while a `.vxtr` file is
//! written, every embedded image is stored once in a top-level `images`
//! list and image nodes refer to it by index; while one is read, the
//! indices are resolved back into bytes. The save also carries the
//! document's preview picture (format 5, spec 14d). Outside [`to_bytes`] and
//! [`from_bytes`] (the clipboard, tests) images stay inline.
//!
//! [`to_bytes`]: super::to_bytes
//! [`from_bytes`]: super::from_bytes

use std::cell::RefCell;
use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Serialize, Serializer};

use crate::hash::fnv1a;

/// The images met so far while saving, each once.
#[derive(Default)]
struct Table {
    images: Vec<Vec<u8>>,
    /// Content hash → indices in `images` with that hash.
    by_hash: HashMap<u64, Vec<u32>>,
    /// The preview PNG, as base64.
    preview: Option<String>,
}

thread_local! {
    static SAVING: RefCell<Option<Table>> = const { RefCell::new(None) };
    static LOADING: RefCell<Option<Vec<Vec<u8>>>> = const { RefCell::new(None) };
}

/// Shares images on this thread until dropped (one save), and writes
/// `preview` as the document's preview.
pub(super) struct SaveScope(());

impl SaveScope {
    pub(super) fn start(preview: Option<&[u8]>) -> Self {
        let table = Table {
            preview: preview.map(|png| STANDARD.encode(png)),
            ..Table::default()
        };
        SAVING.with_borrow_mut(|t| *t = Some(table));
        Self(())
    }
}

impl Drop for SaveScope {
    fn drop(&mut self) {
        SAVING.with_borrow_mut(|t| *t = None);
    }
}

/// Resolves shared image indices on this thread until dropped (one load).
pub(super) struct LoadScope(());

impl LoadScope {
    pub(super) fn start(images: Vec<Vec<u8>>) -> Self {
        LOADING.with_borrow_mut(|t| *t = Some(images));
        Self(())
    }
}

impl Drop for LoadScope {
    fn drop(&mut self) {
        LOADING.with_borrow_mut(|t| *t = None);
    }
}

/// While saving: the index `bytes` is stored at (added when new). `None`
/// outside a save, when images are written inline.
pub(crate) fn share(bytes: &[u8]) -> Option<u32> {
    SAVING.with_borrow_mut(|table| {
        let table = table.as_mut()?;
        let hash = fnv1a(bytes);
        let same = table.by_hash.entry(hash).or_default();
        if let Some(&i) = same
            .iter()
            .find(|&&i| table.images[i as usize].as_slice() == bytes)
        {
            return Some(i);
        }
        let i = u32::try_from(table.images.len()).ok()?;
        table.images.push(bytes.to_vec());
        same.push(i);
        Some(i)
    })
}

/// While loading: the bytes of shared image `index`; `None` when there is
/// no such image (a damaged file) or no load is under way.
pub(crate) fn get(index: u32) -> Option<Vec<u8>> {
    LOADING.with_borrow(|t| t.as_ref()?.get(index as usize).cloned())
}

/// The `images` list of the file being written: every image shared so
/// far, as base64. Serialised after the nodes, so it is complete.
pub(crate) struct Images;

impl Images {
    /// Whether nothing was shared (the list is then left out).
    pub(crate) fn is_empty(_: &Self) -> bool {
        SAVING.with_borrow(|t| t.as_ref().is_none_or(|t| t.images.is_empty()))
    }
}

impl Serialize for Images {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        SAVING.with_borrow(|t| {
            let images = t.as_ref().map_or(&[][..], |t| t.images.as_slice());
            s.collect_seq(images.iter().map(|b| STANDARD.encode(b)))
        })
    }
}

/// The `preview_png` of the file being written, as base64.
pub(crate) struct Preview;

impl Preview {
    /// Whether the save has no preview (the field is then left out).
    pub(crate) fn is_empty(_: &Self) -> bool {
        SAVING.with_borrow(|t| t.as_ref().is_none_or(|t| t.preview.is_none()))
    }
}

impl Serialize for Preview {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        SAVING.with_borrow(|t| {
            let preview = t.as_ref().and_then(|t| t.preview.as_deref());
            s.serialize_str(preview.unwrap_or_default())
        })
    }
}
