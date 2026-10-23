//! SVG ids from node names (spec 10): names are shown as typed in the
//! editor and slugified only when written out.

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

#[cfg(test)]
mod tests {
    use super::slug;

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
}
