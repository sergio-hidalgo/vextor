//! Very big sample documents (spec 14): the sizes the hardening budgets
//! are measured on. Built through [`vextor_core::Command`]s like the other fixtures.

use vextor_core::{
    Container, Document, Geometry, Image, ImageMime, ImageSource, Node, NodeKind, Path, PathNode,
    Rect, Rgba, SubPath, Text, Transform, Vec2,
};

use super::{add_artboard, fill, insert, many_nodes, palette, shape};

/// The node counts measured: up to the 20 000-node file limit
/// (`security-check` §2), counting groups and the layer.
pub const NODE_COUNTS: [usize; 3] = [1_000, 10_000, 20_000];

/// About `total` nodes in all ([`many_nodes`] adds one group per ten
/// shapes, and the document has its layer): the shape count that keeps
/// the whole document within `total`.
#[must_use]
pub fn nodes(total: usize) -> Document {
    // shapes + ceil(shapes / 10) groups + 1 layer ≤ total.
    let shapes = (total.saturating_sub(1) * 10) / 11;
    many_nodes(shapes)
}

/// One open path of `segments` cubic segments, a long wave.
#[must_use]
pub fn long_path(segments: usize) -> Document {
    let mut doc = Document::new("long path");
    add_artboard(&mut doc, "long-path", 2000.0, 2000.0, None);
    let nodes = (0..=segments)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)] // small counts
            let t = i as f64;
            let (x, y) = (t % 2000.0, (t / 2000.0).floor() * 8.0 + 4.0);
            PathNode {
                handle_in: Some(Vec2::new(-0.3, 2.0)),
                handle_out: Some(Vec2::new(0.3, -2.0)),
                ..PathNode::corner(x, y)
            }
        })
        .collect();
    let geometry = Geometry::Path(Path {
        subpaths: vec![SubPath {
            nodes,
            closed: false,
        }],
        fill_rule: vextor_core::FillRule::NonZero,
    });
    let node = shape(doc.alloc_node_id(), geometry, fill(Rgba::rgb(0, 0, 0)));
    insert(&mut doc, vec![node]);
    doc
}

/// `count` embedded images, 64×64 px, drawn from `distinct` different
/// pictures in turn: copies of the same picture are what deduplication
/// stores once.
#[must_use]
pub fn images(count: usize, distinct: usize) -> Document {
    const SIDE: u32 = 64;
    let pictures: Vec<Vec<u8>> = (0..distinct.max(1)).map(|i| noise_png(SIDE, i)).collect();
    let mut doc = Document::new("images");
    add_artboard(&mut doc, "images", 1280.0, 1280.0, None);
    let nodes = (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)] // small counts
            let (x, y) = ((i % 20) as f64 * 64.0, (i / 20) as f64 * 64.0);
            let image = Image {
                rect: Rect::new(x, y, 64.0, 64.0),
                source: ImageSource::Embedded {
                    mime: ImageMime::Png,
                    bytes: pictures[i % pictures.len()].clone(),
                },
            };
            Node::new(doc.alloc_node_id(), NodeKind::Image(image))
        })
        .collect();
    insert(&mut doc, nodes);
    doc
}

/// Groups nested `depth` deep, a small rect in the innermost.
#[must_use]
pub fn deep_groups(depth: usize) -> Document {
    let mut doc = Document::new("deep groups");
    add_artboard(&mut doc, "deep-groups", 200.0, 200.0, None);
    let leaf = shape(
        doc.alloc_node_id(),
        Geometry::Rect {
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            radius: vextor_core::CornerRadii::default(),
        },
        fill(Rgba::rgb(0x1e, 0x90, 0xff)),
    );
    let mut child = leaf.id;
    let mut nodes = vec![leaf];
    for _ in 0..depth {
        let mut group = Node::new(
            doc.alloc_node_id(),
            NodeKind::Group(Container {
                children: vec![child],
                clip: None,
            }),
        );
        group.transform = Transform([1.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        child = group.id;
        nodes.push(group);
    }
    insert(&mut doc, nodes);
    doc
}

/// `count` short text objects in the bundled test family ("Noto Sans"),
/// regular and bold in turn.
#[must_use]
pub fn texts(count: usize) -> Document {
    let mut doc = Document::new("texts");
    add_artboard(&mut doc, "texts", 2000.0, 2000.0, None);
    let nodes = (0..count)
        .map(|i| {
            let mut text = Text::new(format!("Label {i}"), "Noto Sans");
            text.font.weight = if i % 2 == 0 { 400 } else { 700 };
            text.style = fill(palette(i));
            let mut node = Node::new(doc.alloc_node_id(), NodeKind::Text(text));
            #[allow(clippy::cast_precision_loss)] // small counts
            let (x, y) = ((i % 20) as f64 * 100.0, (i / 20) as f64 * 40.0 + 20.0);
            node.transform = Transform([1.0, 0.0, 0.0, 1.0, x, y]);
            node
        })
        .collect();
    insert(&mut doc, nodes);
    doc
}

/// A `side`×`side` PNG of deterministic coloured 8 px blocks, different
/// per `seed`: varied enough not to compress to nothing, and smooth enough
/// that canvas and SVG renderers scale it alike.
fn noise_png(side: u32, seed: usize) -> Vec<u8> {
    const BLOCK: u32 = 8;
    let mut pixmap = tiny_skia::Pixmap::new(side, side).expect("64 px side is non-zero");
    let blocks = side.div_ceil(BLOCK);
    let colours: Vec<[u8; 3]> = {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ u64::try_from(seed).unwrap_or(0);
        (0..blocks * blocks)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let [r, g, b, ..] = state.to_le_bytes();
                [r, g, b]
            })
            .collect()
    };
    for (index, px) in pixmap.pixels_mut().iter_mut().enumerate() {
        let index = u32::try_from(index).unwrap_or(0);
        let (col, row) = (index % side / BLOCK, index / side / BLOCK);
        let [red, green, blue] = colours[(row * blocks + col) as usize];
        *px = tiny_skia::PremultipliedColorU8::from_rgba(red, green, blue, 255)
            .expect("opaque colour is valid premultiplied");
    }
    pixmap.encode_png().expect("PNG encoding of a small pixmap")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_stays_within_the_requested_total() {
        for total in NODE_COUNTS {
            let doc = nodes(total);
            assert!(doc.validate().is_ok());
            assert!(doc.nodes.len() <= total, "{} > {total}", doc.nodes.len());
            assert!(doc.nodes.len() + 12 > total, "close to {total}");
        }
    }

    #[test]
    fn the_other_big_fixtures_are_valid() {
        for doc in [long_path(1_000), images(20, 4), deep_groups(50), texts(40)] {
            assert!(doc.validate().is_ok(), "{}", doc.name);
        }
    }
}
