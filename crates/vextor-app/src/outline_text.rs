//! Object ▸ Convert Text to Outlines (`⇧⌘O`, spec 05c): each selected
//! text becomes a group, named after the text, holding a rect for its
//! frame and one path per letter (named after it) — ordinary shapes that
//! come apart when ungrouped, in one undo step.

use vextor_core::{Command, Container, Node, NodeId, NodeKind, Parent};
use vextor_geom::{MissingFont, text_outlines};

use crate::session::Session;

/// Longest group name taken from the text, in characters.
const NAME_CHARS: usize = 40;

/// Undo label of the conversion.
pub const LABEL: &str = "Convert to Outlines";

/// The selected text objects.
#[must_use]
pub fn selected_texts(s: &Session) -> Vec<NodeId> {
    s.selected()
        .into_iter()
        .filter(|id| {
            matches!(
                s.doc.nodes.get(id).map(|n| &n.kind),
                Some(NodeKind::Text(_))
            )
        })
        .collect()
}

/// The first selected text not drawn in its own font, if any: converting
/// would outline the fallback.
#[must_use]
pub fn missing_font(s: &Session) -> Option<MissingFont> {
    selected_texts(s).into_iter().find_map(|id| {
        let NodeKind::Text(t) = &s.doc.nodes.get(&id)?.kind else {
            return None;
        };
        s.fonts.layout(id, t).missing.clone()
    })
}

/// Converts every selected text to outlines and selects the results.
///
/// # Errors
/// A message for the user when there is no text selected, the fonts have
/// not loaded yet, or the change is refused.
pub fn convert(s: &mut Session) -> Result<(), String> {
    let ids = selected_texts(s);
    if ids.is_empty() {
        return Err(format!("{LABEL}: select a text first"));
    }
    if s.fonts.db().is_empty() {
        return Err(format!(
            "{LABEL}: the fonts are still loading, try again in a moment"
        ));
    }
    let mut commands = Vec::new();
    let mut groups = Vec::new();
    let mut next = s.doc.next_id();
    let mut alloc = || {
        let id = NodeId(next);
        next += 1;
        id
    };
    for id in ids {
        let Some(node) = s.doc.nodes.get(&id) else {
            continue;
        };
        let NodeKind::Text(text) = &node.kind else {
            continue;
        };
        let Some(Parent::Node(parent)) = s.doc.parent_of(id) else {
            continue;
        };
        let index = s
            .doc
            .nodes
            .get(&parent)
            .and_then(|p| p.container())
            .and_then(|c| c.children.iter().position(|&c| c == id))
            .unwrap_or(0);
        let layout = s.fonts.layout(id, text);
        let (frame, glyphs) = text_outlines(text, &layout);
        let group_id = alloc();
        let mut children = Vec::new();
        let mut inserted = Vec::new();
        let shapes = frame
            .map(|f| (None, f))
            .into_iter()
            .chain(glyphs.into_iter().map(|(name, g)| (Some(name), g)));
        for (name, shape) in shapes {
            let child = alloc();
            children.push(child);
            let mut leaf = Node::new(child, NodeKind::Shape(shape));
            leaf.name = name.as_deref().and_then(vextor_core::clean_name);
            inserted.push(leaf);
        }
        let group = Node {
            id: group_id,
            name: vextor_core::clean_name(&group_name(&text.content)),
            kind: NodeKind::Group(Container {
                children,
                clip: None,
            }),
            ..node.clone()
        };
        inserted.insert(0, group);
        commands.push(Command::ReplaceWith {
            removed: vec![id],
            inserted,
            parent: Some(parent),
            index,
        });
        groups.push(group_id);
    }
    s.apply(Command::Batch {
        label: LABEL,
        commands,
    })
    .map_err(|e| format!("{LABEL} failed: {e}"))?;
    s.select(groups);
    Ok(())
}

/// The text's first line, trimmed and shortened; "Text" when blank.
fn group_name(content: &str) -> String {
    let line = content.lines().map(str::trim).find(|l| !l.is_empty());
    match line {
        Some(l) if l.chars().count() > NAME_CHARS => {
            let cut: String = l.chars().take(NAME_CHARS).collect();
            format!("{}…", cut.trim_end())
        }
        Some(l) => l.to_owned(),
        None => "Text".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vextor_core::{CornerRadii, Geometry, Paint, Rgba, Style, Text, TextFrame, Transform};
    use vextor_geom::{FontDb, Fonts};

    use super::*;
    use crate::new_doc::new_document;

    const REGULAR: &[u8] =
        include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");

    fn label(family: &str) -> Text {
        Text {
            style: Style {
                fill: Paint::Solid(Rgba::rgb(255, 255, 255)),
                stroke: None,
            },
            frame: Some(TextFrame {
                padding: [8.0, 16.0, 8.0, 16.0],
                radius: CornerRadii {
                    tl: 6.0,
                    tr: 6.0,
                    br: 6.0,
                    bl: 6.0,
                },
                fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
                stroke: None,
            }),
            ..Text::new("Buy now\nsecond line", family)
        }
    }

    fn session(texts: Vec<Text>) -> (Session, Vec<NodeId>) {
        let mut doc = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        let ids: Vec<NodeId> = texts.iter().map(|_| doc.alloc_node_id()).collect();
        let nodes = ids
            .iter()
            .zip(texts)
            .map(|(&id, t)| {
                let mut n = Node::new(id, NodeKind::Text(t));
                n.transform = Transform([1.0, 0.0, 0.0, 1.0, 20.0, 30.0]);
                n.opacity = 0.5;
                n
            })
            .collect();
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.fonts = Arc::new(Fonts::new(Arc::new(FontDb::from_data([REGULAR.to_vec()]))));
        s.select(ids.clone());
        (s, ids)
    }

    #[test]
    fn a_text_becomes_a_named_group_of_rect_and_glyph_path() {
        let (mut s, ids) = session(vec![label("Noto Sans")]);
        let layer = s.doc.layers[0];
        convert(&mut s).expect("converts");
        assert!(!s.doc.nodes.contains_key(&ids[0]));
        let [g] = s.selected()[..] else {
            panic!("the group is selected");
        };
        let group = &s.doc.nodes[&g];
        assert_eq!(group.name.as_deref(), Some("Buy now"));
        assert_eq!(group.transform, Transform([1.0, 0.0, 0.0, 1.0, 20.0, 30.0]));
        assert!(
            (group.opacity - 0.5).abs() < 1e-9,
            "node properties carry over"
        );
        let children = group.container().expect("group").children.clone();
        // The frame, then "Buynowsecondline": a path per letter, none for
        // the space.
        assert_eq!(children.len(), 1 + 16);
        let NodeKind::Shape(rect) = &s.doc.nodes[&children[0]].kind else {
            panic!("frame first");
        };
        assert!(matches!(rect.geometry, Geometry::Rect { .. }));
        assert_eq!(rect.style.fill, Paint::Solid(Rgba::rgb(30, 144, 255)));
        let letter = &s.doc.nodes[&children[1]];
        assert_eq!(letter.name.as_deref(), Some("B"));
        let NodeKind::Shape(path) = &letter.kind else {
            panic!("glyphs on top");
        };
        let Geometry::Path(p) = &path.geometry else {
            panic!("a path");
        };
        assert_eq!(p.subpaths.len(), 3, "B: outline and two holes");
        assert_eq!(path.style.fill, Paint::Solid(Rgba::rgb(255, 255, 255)));
        assert_eq!(
            s.doc.nodes[&layer].container().expect("layer").children,
            [g]
        );

        assert_eq!(s.edit_menu().undo, Some(LABEL));
        s.undo();
        assert!(
            s.doc.nodes.contains_key(&ids[0]),
            "one step back to the text"
        );
        assert!(!s.doc.nodes.contains_key(&g));
    }

    #[test]
    fn several_texts_keep_their_stacking_order() {
        let mut plain = label("Noto Sans");
        plain.frame = None;
        let (mut s, _) = session(vec![label("Noto Sans"), plain]);
        let layer = s.doc.layers[0];
        convert(&mut s).expect("converts");
        let groups = s.selected();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            s.doc.nodes[&layer].container().expect("layer").children,
            groups
        );
        let unframed = s.doc.nodes[&groups[1]]
            .container()
            .expect("group")
            .children
            .len();
        assert_eq!(unframed, 16, "no frame, only the letters");
        s.doc.validate().expect("valid");
    }

    #[test]
    fn ungrouping_the_result_frees_every_letter() {
        let mut t = label("Noto Sans");
        t.frame = None;
        t.content = "Hi".to_owned();
        let (mut s, _) = session(vec![t]);
        let layer = s.doc.layers[0];
        convert(&mut s).expect("converts");
        crate::edit_ops::ungroup(&mut s).expect("ungroups");
        let letters = s.doc.nodes[&layer]
            .container()
            .expect("layer")
            .children
            .clone();
        assert_eq!(letters.len(), 2, "H and i, each its own object");
        let names: Vec<_> = letters
            .iter()
            .map(|id| s.doc.nodes[id].name.clone().unwrap_or_default())
            .collect();
        assert_eq!(names, ["H", "i"]);
        // The group's place carried into each letter.
        let world = s.doc.world_transform(letters[0]);
        assert_eq!((world.0[4], world.0[5]), (20.0, 30.0));
    }

    #[test]
    fn a_missing_font_is_reported_before_converting() {
        let (s, _) = session(vec![label("Nope Display")]);
        let m = missing_font(&s).expect("missing");
        assert_eq!(m.wanted, "Nope Display");
        assert_eq!(m.shown.as_deref(), Some("Noto Sans"));
        let (s, _) = session(vec![label("Noto Sans")]);
        assert_eq!(missing_font(&s), None);
    }

    #[test]
    fn nothing_converts_without_text_or_fonts() {
        let (mut s, _) = session(vec![label("Noto Sans")]);
        s.select(Vec::new());
        assert!(convert(&mut s).is_err());
        let (mut s, _) = session(vec![label("Noto Sans")]);
        s.fonts = Arc::default();
        assert!(convert(&mut s).is_err());
        assert_eq!(s.edit_menu().undo, None, "nothing was recorded");
    }

    #[test]
    fn group_names_come_from_the_first_line() {
        assert_eq!(group_name("  Buy now \nmore"), "Buy now");
        assert_eq!(group_name("\n\n"), "Text");
        assert_eq!(group_name(&"a".repeat(50)), format!("{}…", "a".repeat(40)));
    }

    #[test]
    fn outlining_text_with_a_tab_gives_valid_names() {
        let text = Text {
            content: "Pay\tnow".to_owned(),
            ..label("Noto Sans")
        };
        let (mut s, _) = session(vec![text]);
        convert(&mut s).expect("converts");
        let [g] = s.selected()[..] else {
            panic!("the group is selected");
        };
        assert_eq!(s.doc.nodes[&g].name.as_deref(), Some("Pay now"));
        s.doc.validate().expect("every name is valid");
    }
}
