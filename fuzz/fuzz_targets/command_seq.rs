//! Command sequences (spec 14): any sequence of edits, undos and redos
//! keeps the document valid after every step (a rejected command changes
//! nothing), and the result saves and loads back equal.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vextor_core::{
    Command, CornerRadii, Document, Geometry, History, Node, NodeId, NodeKind, Rect, Selection,
    Shape, Style, Transform, from_bytes, to_bytes,
};

/// Reads small numbers from the fuzzer's bytes.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn next(&mut self) -> Option<u8> {
        let (&b, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(b)
    }

    fn float(&mut self) -> f64 {
        f64::from(i8::from_ne_bytes([self.next().unwrap_or(0)])) * 2.5
    }
}

fn pick(ids: &[NodeId], b: u8) -> Option<NodeId> {
    (!ids.is_empty()).then(|| ids[usize::from(b) % ids.len()])
}

/// Every node but the layers, in id order.
fn objects(doc: &Document) -> Vec<NodeId> {
    doc.nodes
        .values()
        .filter(|n| !matches!(n.kind, NodeKind::Layer(_)))
        .map(|n| n.id)
        .collect()
}

fn rect(doc: &mut Document, bytes: &mut Bytes) -> Node {
    let (x, y) = (bytes.float(), bytes.float());
    let side = bytes.float().abs() + 1.0;
    Node::new(
        doc.alloc_node_id(),
        NodeKind::Shape(Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(x, y, side, side),
                radius: CornerRadii::default(),
            },
            style: Style::default(),
        }),
    )
}

fuzz_target!(|data: &[u8]| {
    let mut bytes = Bytes(data);
    let mut doc = Document::new("fuzz");
    let mut history = History::new();
    let selection = Selection::default();
    while let Some(kind) = bytes.next() {
        let a = bytes.next().unwrap_or(0);
        let b = bytes.next().unwrap_or(0);
        let ids = objects(&doc);
        let layer = doc.layers[0];
        let cmd = match kind % 10 {
            0 | 1 => {
                let node = rect(&mut doc, &mut bytes);
                let parent = pick(&ids, a)
                    .filter(|id| matches!(doc.nodes[id].kind, NodeKind::Group(_)))
                    .unwrap_or(layer);
                Command::Insert {
                    parent: Some(parent),
                    index: usize::from(b),
                    nodes: vec![node],
                }
            }
            2 => match pick(&ids, a) {
                Some(id) => Command::Delete { ids: vec![id] },
                None => continue,
            },
            3 => {
                let mut group: Vec<NodeId> = ids.iter().copied().filter(|_| bytes.next().unwrap_or(0) % 2 == 0).collect();
                group.dedup();
                Command::Group { ids: group }
            }
            4 => match pick(&ids, a) {
                Some(id) => Command::Ungroup { id },
                None => continue,
            },
            5 => match pick(&ids, a) {
                Some(id) => Command::SetTransform {
                    ids: vec![id],
                    transform: Transform([1.0, 0.0, 0.0, 1.0, bytes.float(), bytes.float()]),
                },
                None => continue,
            },
            6 => match (pick(&ids, a), pick(&ids, b)) {
                (Some(id), target) => Command::Reparent {
                    ids: vec![id],
                    new_parent: target.or(Some(layer)),
                    index: usize::from(bytes.next().unwrap_or(0)),
                },
                _ => continue,
            },
            7 => match pick(&ids, a) {
                Some(id) => Command::SetProps {
                    id,
                    name: (b % 3 == 0).then(|| format!("n{b}")),
                    visible: b % 2 == 0,
                    locked: b % 5 == 0,
                    opacity: f64::from(b) / 255.0,
                },
                None => continue,
            },
            8 => {
                history.undo(&mut doc);
                assert!(doc.validate().is_ok(), "valid after undo");
                continue;
            }
            _ => {
                history.redo(&mut doc);
                assert!(doc.validate().is_ok(), "valid after redo");
                continue;
            }
        };
        let before = doc.clone();
        if history.apply(&mut doc, cmd, &selection).is_err() {
            assert_eq!(doc, before, "a rejected command changes nothing");
        }
        assert!(doc.validate().is_ok(), "{:?}", doc.validate());
    }
    let saved = to_bytes(&doc).expect("a valid document saves");
    assert_eq!(from_bytes(&saved).expect("and loads"), doc);
});
