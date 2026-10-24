//! SVG ids from node names (spec 10): names are shown as typed in the
//! editor and slugified only when written out.

use std::collections::BTreeSet;

/// `name` as a readable SVG id: lowercase ASCII letters and digits, words
/// joined by `-`, starting with a letter ("Primary Button!" →
/// `primary-button`, "24px icon" → `id-24px-icon`). `None` when nothing
/// usable is left. Deduplicating is the exporter's job.
#[must_use]
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            gap = false;
        } else {
            gap = true;
        }
    }
    if out.is_empty() {
        return None;
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "id-");
    }
    Some(out)
}

/// The ids (and class names) one export hands out: each unique, all
/// starting with the same prefix (empty, or `vx-3f2a-` for unique ids).
#[derive(Debug, Default)]
pub(crate) struct Ids {
    prefix: String,
    used: BTreeSet<String>,
}

impl Ids {
    pub(crate) fn new(prefix: String) -> Self {
        Self {
            prefix,
            used: BTreeSet::new(),
        }
    }

    pub(crate) fn prefix(&self) -> &str {
        &self.prefix
    }

    /// A new id from `base` (a slug): `base`, else `base-2`, `base-3`…
    pub(crate) fn take(&mut self, base: &str) -> String {
        let mut id = format!("{}{base}", self.prefix);
        let mut n = 2;
        while self.used.contains(&id) {
            id = format!("{}{base}-{n}", self.prefix);
            n += 1;
        }
        self.used.insert(id.clone());
        id
    }

    /// A numbered id for unnamed definitions: `base-1`, `base-2`…
    pub(crate) fn numbered(&mut self, base: &str) -> String {
        let mut n = 1;
        loop {
            let id = format!("{}{base}-{n}", self.prefix);
            if self.used.insert(id.clone()) {
                return id;
            }
            n += 1;
        }
    }
}

/// The prefix of unique ids: `vx-` and four hex digits hashed (FNV-1a)
/// from `key`, so the same document and target always get the same one.
pub(crate) fn unique_prefix(key: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in key.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("vx-{:04x}-", (h >> 16) ^ (h & 0xffff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_readable_ids() {
        assert_eq!(slug("Primary Button!").as_deref(), Some("primary-button"));
        assert_eq!(slug("  icon__close  ").as_deref(), Some("icon-close"));
        assert_eq!(slug("Layer 1").as_deref(), Some("layer-1"));
        assert_eq!(slug("24px icon").as_deref(), Some("id-24px-icon"));
        assert_eq!(slug("café <b>").as_deref(), Some("caf-b"));
    }

    #[test]
    fn nothing_usable_gives_no_id() {
        assert_eq!(slug(""), None);
        assert_eq!(slug("¡¿ — !?"), None);
        assert_eq!(slug("日本"), None);
    }

    #[test]
    fn ids_are_unique_and_prefixed() {
        let mut ids = Ids::new("vx-0001-".to_owned());
        assert_eq!(ids.take("star"), "vx-0001-star");
        assert_eq!(ids.take("star"), "vx-0001-star-2");
        assert_eq!(ids.numbered("star"), "vx-0001-star-1");
        assert_eq!(ids.numbered("star"), "vx-0001-star-3");
    }

    #[test]
    fn unique_prefixes_are_stable_and_differ_by_key() {
        assert_eq!(unique_prefix("a"), unique_prefix("a"));
        assert_ne!(unique_prefix("icon"), unique_prefix("button"));
        assert_eq!(unique_prefix("a").len(), "vx-0000-".len());
    }
}
