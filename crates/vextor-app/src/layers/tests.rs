//! The Layers panel's rules, without a window.

use vextor_core::{
    Command, Container, CornerRadii, Document, Geometry, Node, NodeId, NodeKind, Rect, Shape,
    Style, Text,
};

use super::*;
use crate::edit_ops;
use crate::new_doc::new_document;

fn rect(id: NodeId, x: f64) -> Node {
    Node::new(
        id,
        NodeKind::Shape(Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(x, 0.0, 10.0, 10.0),
                radius: CornerRadii::default(),
            },
            style: Style::default(),
        }),
    )
}

/// Layer 1 holds [a, group(b, c), text]; Layer 2 holds [d].
struct Fx {
    s: Session,
    layer1: NodeId,
    layer2: NodeId,
    a: NodeId,
    group: NodeId,
    b: NodeId,
    c: NodeId,
    text: NodeId,
    d: NodeId,
}

fn fx() -> Fx {
    let mut doc = new_document("t", 100.0, 100.0);
    let layer1 = doc.layers[0];
    let [layer2, a, group, b, c, text, d] = [(); 7].map(|()| doc.alloc_node_id());
    let group_node = Node::new(
        group,
        NodeKind::Group(Container {
            children: vec![b, c],
            clip: None,
        }),
    );
    let text_node = Node::new(text, NodeKind::Text(Text::new("Buy now\nsecond", "Inter")));
    doc.apply(Command::Insert {
        parent: Some(layer1),
        index: 0,
        nodes: vec![
            rect(a, 0.0),
            group_node,
            rect(b, 20.0),
            rect(c, 40.0),
            text_node,
        ],
    })
    .expect("inserts");
    let mut layer = Node::new(layer2, NodeKind::Layer(Container::default()));
    layer.name = Some("Layer 2".into());
    doc.apply(Command::Insert {
        parent: None,
        index: 1,
        nodes: vec![layer],
    })
    .expect("adds a layer");
    doc.apply(Command::Insert {
        parent: Some(layer2),
        index: 0,
        nodes: vec![rect(d, 60.0)],
    })
    .expect("inserts");
    Fx {
        s: Session::new(doc),
        layer1,
        layer2,
        a,
        group,
        b,
        c,
        text,
        d,
    }
}

/// Builds a row edit on the document and applies it.
fn run(s: &mut Session, make: impl FnOnce(&Document) -> Option<Command>) {
    let cmd = make(&s.doc);
    assert!(cmd.is_some(), "the edit changes something");
    apply(s, cmd).expect("applies");
}

fn ids(rows: &[Row]) -> Vec<NodeId> {
    rows.iter().map(|r| r.id).collect()
}

fn children(s: &Session, id: NodeId) -> Vec<NodeId> {
    s.doc.nodes[&id]
        .container()
        .expect("container")
        .children
        .clone()
}

// ----- rows -----

#[test]
fn rows_list_the_top_of_the_stack_first_with_groups_closed() {
    let f = fx();
    let rows = rows(&f.s);
    assert_eq!(
        ids(&rows),
        vec![f.layer2, f.d, f.layer1, f.text, f.group, f.a]
    );
    let group = &rows[4];
    assert!(group.container && !group.open);
    assert_eq!(group.depth, 1);
}

#[test]
fn opening_a_group_lists_its_children() {
    let mut f = fx();
    f.s.layers_panel.toggle(f.group);
    assert_eq!(ids(&rows(&f.s))[4..].to_vec(), vec![f.group, f.c, f.b, f.a]);
    f.s.layers_panel.toggle(f.layer1);
    assert_eq!(ids(&rows(&f.s)), vec![f.layer2, f.d, f.layer1]);
}

#[test]
fn text_rows_show_a_t_and_their_first_line_until_renamed() {
    let mut f = fx();
    let row = rows(&f.s)
        .into_iter()
        .find(|r| r.id == f.text)
        .expect("listed");
    assert_eq!(row.icon, Icon::ToolText);
    assert_eq!(row.label, "Buy now");
    assert!(!row.named);
    let cmd = rename(&f.s.doc, f.text, "CTA label");
    apply(&mut f.s, cmd).expect("renames");
    let row = rows(&f.s)
        .into_iter()
        .find(|r| r.id == f.text)
        .expect("listed");
    assert_eq!((row.label.as_str(), row.named), ("CTA label", true));
}

#[test]
fn default_labels_say_what_the_object_is() {
    let mut text = Node::new(NodeId(1), NodeKind::Text(Text::new("   ", "Inter")));
    assert_eq!(label(&text), "Text");
    text.kind = NodeKind::Text(Text::new("long ".repeat(20), "Inter"));
    let long = label(&text);
    assert!(long.ends_with('…') && long.chars().count() <= TEXT_LABEL_CHARS);
    assert_eq!(label(&rect(NodeId(2), 0.0)), "Rectangle");
}

// ----- clicks and the current layer -----

#[test]
fn clicking_a_row_selects_and_shift_adds() {
    let mut f = fx();
    click(&mut f.s, f.a, false);
    assert_eq!(f.s.selected(), vec![f.a]);
    click(&mut f.s, f.b, true);
    assert_eq!(f.s.selected(), vec![f.a, f.b]);
    click(&mut f.s, f.a, true);
    assert_eq!(f.s.selected(), vec![f.b]);
    assert_eq!(current_layer(&f.s), Some(f.layer1));
}

#[test]
fn a_layer_row_becomes_where_new_objects_go() {
    let mut f = fx();
    click(&mut f.s, f.a, false);
    click(&mut f.s, f.layer1, false);
    assert_eq!(f.s.selected(), Vec::<NodeId>::new());
    assert_eq!(
        edit_ops::paste_target(&f.s),
        Some((f.layer1, 3)),
        "into Layer 1, not the top layer"
    );
}

#[test]
fn a_hidden_or_locked_current_layer_falls_back_to_the_top_open_one() {
    let mut f = fx();
    click(&mut f.s, f.layer1, false);
    run(&mut f.s, |d| toggle_locked(d, f.layer1));
    assert_eq!(edit_ops::paste_target(&f.s), Some((f.layer2, 1)));
}

#[test]
fn locked_or_hidden_rows_are_not_selected() {
    let mut f = fx();
    run(&mut f.s, |d| toggle_locked(d, f.a));
    click(&mut f.s, f.a, false);
    assert_eq!(f.s.selected(), Vec::<NodeId>::new());
    run(&mut f.s, |d| toggle_visible(d, f.layer2));
    click(&mut f.s, f.d, false);
    assert_eq!(f.s.selected(), Vec::<NodeId>::new());
}

#[test]
fn hiding_or_locking_a_selected_object_deselects_it() {
    let mut f = fx();
    f.s.select(vec![f.a, f.d]);
    run(&mut f.s, |d| toggle_visible(d, f.a));
    assert_eq!(f.s.selected(), vec![f.d]);
    run(&mut f.s, |d| toggle_locked(d, f.layer2));
    assert_eq!(f.s.selected(), Vec::<NodeId>::new());
    assert!(f.s.undo());
    assert!(!f.s.doc.nodes[&f.layer2].locked);
}

#[test]
fn selecting_on_the_canvas_reveals_the_row() {
    let mut f = fx();
    f.s.select(vec![f.c]);
    reveal(&mut f.s);
    assert!(f.s.layers_panel.is_open(&f.s.doc, f.group));
    assert_eq!(f.s.layers_panel.scroll_to, Some(f.c));
    assert_eq!(current_layer(&f.s), Some(f.layer1));
    assert!(ids(&rows(&f.s)).contains(&f.c));
}

// ----- rename, opacity -----

#[test]
fn rename_keeps_what_was_typed_and_blank_restores_the_default() {
    let mut f = fx();
    let cmd = rename(&f.s.doc, f.a, "  Primary Button!  ");
    apply(&mut f.s, cmd).expect("renames");
    assert_eq!(f.s.doc.nodes[&f.a].name.as_deref(), Some("Primary Button!"));
    assert!(
        rename(&f.s.doc, f.a, "Primary Button!").is_none(),
        "unchanged"
    );
    let cmd = rename(&f.s.doc, f.a, "   ");
    apply(&mut f.s, cmd).expect("clears");
    assert_eq!(f.s.doc.nodes[&f.a].name, None);
    assert!(f.s.undo());
    assert_eq!(f.s.doc.nodes[&f.a].name.as_deref(), Some("Primary Button!"));
}

#[test]
fn layer_opacity_changes_only_that_layer() {
    let mut f = fx();
    run(&mut f.s, |d| set_opacity(d, f.layer1, 0.3));
    run(&mut f.s, |d| set_opacity(d, f.layer2, 0.7));
    assert!((f.s.doc.nodes[&f.layer1].opacity - 0.3).abs() < 1e-12);
    assert!((f.s.doc.nodes[&f.layer2].opacity - 0.7).abs() < 1e-12);
    run(&mut f.s, |d| set_opacity(d, f.layer2, 2.0));
    assert!((f.s.doc.nodes[&f.layer2].opacity - 1.0).abs() < 1e-12);
    assert!((f.s.doc.nodes[&f.layer1].opacity - 0.3).abs() < 1e-12);
}

// ----- layer buttons -----

#[test]
fn new_duplicate_and_delete_layer_are_undo_steps() {
    let mut f = fx();
    click(&mut f.s, f.layer1, false);
    new_layer(&mut f.s).expect("adds");
    let added = current_layer(&f.s).expect("current");
    assert_eq!(f.s.doc.layers, vec![f.layer1, added, f.layer2]);
    assert_eq!(f.s.edit_menu().undo, Some("New layer"));

    click(&mut f.s, f.layer2, false);
    duplicate_layer(&mut f.s).expect("duplicates");
    let copy = current_layer(&f.s).expect("current");
    assert_eq!(f.s.doc.layers.last(), Some(&copy));
    assert_eq!(children(&f.s, copy).len(), 1);
    assert_ne!(children(&f.s, copy), vec![f.d]);

    delete_layer(&mut f.s).expect("deletes");
    assert_eq!(f.s.doc.layers, vec![f.layer1, added, f.layer2]);
    assert_eq!(current_layer(&f.s), Some(f.layer2));
    assert!(f.s.undo());
    assert!(f.s.doc.layers.contains(&copy));
}

#[test]
fn the_last_layer_is_kept() {
    let mut s = Session::new(new_document("t", 10.0, 10.0));
    assert!(delete_layer(&mut s).is_err());
    assert_eq!(s.doc.layers.len(), 1);
}

// ----- drag and drop -----

#[test]
fn drop_zones_split_rows() {
    let f = fx();
    let rows = rows(&f.s);
    let group = rows.iter().find(|r| r.id == f.group).expect("listed");
    let leaf = rows.iter().find(|r| r.id == f.a).expect("listed");
    assert_eq!(drop_at(group, 0.1), Drop::Above(f.group));
    assert_eq!(drop_at(group, 0.5), Drop::Into(f.group));
    assert_eq!(drop_at(group, 0.9), Drop::Below(f.group));
    assert_eq!(drop_at(leaf, 0.4), Drop::Above(f.a));
    assert_eq!(drop_at(leaf, 0.6), Drop::Below(f.a));
}

#[test]
fn dragging_a_layer_reorders_layers() {
    let mut f = fx();
    drop(&mut f.s, &[f.layer2], Drop::Below(f.layer1)).expect("moves");
    assert_eq!(f.s.doc.layers, vec![f.layer2, f.layer1]);
    assert!(f.s.undo());
    assert_eq!(f.s.doc.layers, vec![f.layer1, f.layer2]);
}

#[test]
fn dragging_objects_reorders_and_reparents() {
    let mut f = fx();
    // Above the text = on top of Layer 1.
    drop(&mut f.s, &[f.a], Drop::Above(f.text)).expect("moves");
    assert_eq!(children(&f.s, f.layer1), vec![f.group, f.text, f.a]);
    // Onto a group: inside it, on top.
    drop(&mut f.s, &[f.d], Drop::Into(f.group)).expect("moves");
    assert_eq!(children(&f.s, f.group), vec![f.b, f.c, f.d]);
    // Onto a layer row: inside that layer.
    drop(&mut f.s, &[f.b], Drop::Above(f.layer2)).expect("moves");
    assert_eq!(children(&f.s, f.layer2), vec![f.b]);
}

#[test]
fn drops_that_cannot_be_are_refused() {
    let f = fx();
    let doc = &f.s.doc;
    assert!(drop_command(doc, &[f.layer2], Drop::Into(f.group)).is_none());
    assert!(drop_command(doc, &[f.layer2], Drop::Above(f.a)).is_none());
    assert!(drop_command(doc, &[f.group], Drop::Into(f.group)).is_none());
    assert!(drop_command(doc, &[f.group], Drop::Above(f.b)).is_none());
    assert!(drop_command(doc, &[f.layer1, f.d], Drop::Above(f.layer2)).is_none());
    assert!(drop_command(doc, &[], Drop::Above(f.a)).is_none());
}

#[test]
fn a_drag_from_a_selected_row_carries_the_selection() {
    let mut f = fx();
    f.s.select(vec![f.a, f.text]);
    assert_eq!(dragged(&f.s, f.text), vec![f.a, f.text]);
    assert_eq!(dragged(&f.s, f.d), vec![f.d]);
}

// ----- clipping -----

#[test]
fn make_clip_and_release_clip_from_the_object_menu() {
    let mut f = fx();
    f.s.select(vec![f.text, f.a]);
    assert!(!edit_ops::can(&f.s).make_clip, "text on top cannot clip");
    f.s.select(vec![f.b, f.c]);
    assert!(edit_ops::can(&f.s).make_clip);
    edit_ops::make_clip(&mut f.s).expect("clips");
    let clip_group = f.s.selected()[0];
    let row_label = label(&f.s.doc.nodes[&clip_group]);
    assert_eq!(row_label, "Clip group");
    f.s.layers_panel.toggle(clip_group);
    f.s.layers_panel.toggle(f.group);
    let clip_row = rows(&f.s)
        .into_iter()
        .find(|r| r.id == f.c)
        .expect("listed");
    assert!(clip_row.clip);

    assert!(edit_ops::can(&f.s).release_clip);
    edit_ops::release_clip(&mut f.s).expect("releases");
    assert!(!edit_ops::can(&f.s).release_clip);
    assert!(f.s.undo());
    assert!(edit_ops::can(&f.s).release_clip);
}
