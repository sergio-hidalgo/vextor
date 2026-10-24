//! Image to Vector (spec 13b) on fixtures drawn here, antialiased like a
//! real export: a black logo on white, a 3-colour icon, a 6-colour
//! illustration, a cut-out cartoon face on a transparent background (the
//! owner's review case) and a photo-like image (gradients, noise, soft
//! blobs), so no third-party file is needed.

use std::collections::HashMap;

use vextor_core::{Document, Vec2, View};
use vextor_geom::Fonts;
use vextor_render::{Pixmap, Viewport, render, rgba_png};
use vextor_svg::{Format, Scope};

use super::*;

/// `w` × `h` straight RGBA: `paint` sampled 4 × 4 times per pixel and
/// averaged, so edges are antialiased.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn draw(w: u32, h: u32, paint: impl Fn(f64, f64) -> [u8; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0_u32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let c = paint(
                        f64::from(x) + (f64::from(sx) + 0.5) / 4.0,
                        f64::from(y) + (f64::from(sy) + 0.5) / 4.0,
                    );
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += u32::from(v);
                    }
                }
            }
            out.extend(sum.map(|s| ((s + 8) / 16) as u8));
            out.push(255);
        }
    }
    out
}

/// [`draw`] with transparency: `paint` gives `None` where nothing is
/// drawn; edges get partial alpha, colours stay straight.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn draw_cutout(w: u32, h: u32, paint: impl Fn(f64, f64) -> Option<[u8; 3]>) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut hits) = ([0_u32; 3], 0_u32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let c = paint(
                        f64::from(x) + (f64::from(sx) + 0.5) / 4.0,
                        f64::from(y) + (f64::from(sy) + 0.5) / 4.0,
                    );
                    if let Some(c) = c {
                        hits += 1;
                        for (s, v) in sum.iter_mut().zip(c) {
                            *s += u32::from(v);
                        }
                    }
                }
            }
            let color = sum.map(|s| (s + hits / 2).checked_div(hits).unwrap_or(0) as u8);
            out.extend(color);
            out.push((hits * 255 / 16) as u8);
        }
    }
    out
}

/// Inside the ellipse at (`cx`, `cy`) with radii `rx`, `ry`.
fn inside_ellipse(x: f64, y: f64, (cx, cy, rx, ry): (f64, f64, f64, f64)) -> bool {
    ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0
}

/// A cut-out cartoon face on transparency, in the colours of the owner's
/// review image: black outline, green head and ears with darker ear spots,
/// a lime snout with a mid-dark green ring and dark nostrils, white eyes
/// with dark green rings and black pupils.
fn cartoon() -> (u32, u32, Vec<u8>) {
    const OUTLINE: [u8; 3] = [0, 0, 0];
    const BODY: [u8; 3] = [0x6b, 0xe2, 0x48];
    const EAR_SPOT: [u8; 3] = [0x46, 0xb3, 0x28];
    const SNOUT: [u8; 3] = [0xa5, 0xe8, 0x01];
    const SNOUT_RING: [u8; 3] = [0x4d, 0x8e, 0x00];
    const NOSTRIL: [u8; 3] = [0x13, 0x38, 0x0e];
    const EYE_RING: [u8; 3] = [0x43, 0x75, 0x18];
    let grow = |(cx, cy, rx, ry): (f64, f64, f64, f64), by: f64| (cx, cy, rx + by, ry + by);
    let head = (200.0, 220.0, 150.0, 130.0);
    // ears above the head, each spot well inside its ear
    let ears = [(120.0, 70.0, 35.0, 35.0), (280.0, 70.0, 35.0, 35.0)];
    let snout = (200.0, 235.0, 70.0, 50.0);
    let eyes = [(95.0, 215.0, 32.0, 32.0), (305.0, 215.0, 32.0, 32.0)];
    let px = draw_cutout(400, 380, |x, y| {
        let at = |e| inside_ellipse(x, y, e);
        if at(head) {
            for (cx, cy, ..) in eyes {
                let eye = (cx, cy, 32.0, 32.0);
                if at(grow(eye, -27.0)) {
                    return Some(OUTLINE); // pupil, 5 px
                }
                if at(grow(eye, -6.0)) {
                    return Some(WHITE);
                }
                if at(eye) {
                    return Some(EYE_RING);
                }
            }
            for nostril in [(175.0, 235.0, 10.0, 16.0), (225.0, 235.0, 10.0, 16.0)] {
                if at(nostril) {
                    return Some(NOSTRIL);
                }
            }
            if at(grow(snout, -7.0)) {
                return Some(SNOUT);
            }
            if at(snout) {
                return Some(SNOUT_RING);
            }
            return Some(if at(grow(head, -10.0)) { BODY } else { OUTLINE });
        }
        for ear in ears {
            if at(grow(ear, -21.0)) {
                return Some(EAR_SPOT);
            }
            if at(grow(ear, -8.0)) {
                return Some(BODY);
            }
            if at(ear) {
                return Some(OUTLINE);
            }
        }
        None
    });
    (400, 380, px)
}

/// The shortest distance from `p` to the outlines of `shapes`, in px.
fn distance_to(shapes: &[Shape], p: kurbo::Point) -> f64 {
    use kurbo::ParamCurveNearest;
    shapes
        .iter()
        .flat_map(|shape| {
            let Geometry::Path(path) = &shape.geometry else {
                panic!("a path");
            };
            vextor_geom::path_to_bezpath(path)
                .segments()
                .collect::<Vec<_>>()
        })
        .map(|seg| seg.nearest(p, 1e-6).distance_sq.sqrt())
        .fold(f64::INFINITY, f64::min)
}

/// The fill colours of `shapes`, each once.
fn colours(shapes: &[Shape]) -> Vec<Paint> {
    let mut out: Vec<Paint> = Vec::new();
    for sh in shapes {
        if !out.contains(&sh.style.fill) {
            out.push(sh.style.fill);
        }
    }
    out
}

fn inside_circle(x: f64, y: f64, cx: f64, cy: f64, r: f64) -> bool {
    (x - cx).hypot(y - cy) <= r
}

const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];

/// A ring over a bar, black on white.
fn logo() -> (u32, u32, Vec<u8>) {
    let px = draw(240, 200, |x, y| {
        let d = (x - 120.0).hypot(y - 80.0);
        let ring = (40.0..65.0).contains(&d);
        let bar = (40.0..200.0).contains(&x) && (160.0..180.0).contains(&y);
        if ring || bar { BLACK } else { WHITE }
    });
    (240, 200, px)
}

/// A blue rounded square with a yellow dot, on white.
fn icon() -> (u32, u32, Vec<u8>) {
    let px = draw(256, 256, |x, y| {
        if inside_circle(x, y, 128.0, 128.0, 50.0) {
            return [250, 200, 20];
        }
        // rounded square 32..224, corner radius 40
        let (qx, qy) = ((x - 128.0).abs() - 56.0, (y - 128.0).abs() - 56.0);
        let outside = qx.max(0.0).hypot(qy.max(0.0));
        if qx.max(qy) <= 40.0 && outside <= 40.0 {
            [30, 70, 200]
        } else {
            WHITE
        }
    });
    (256, 256, px)
}

/// Sky, ground, sun, a wall, a roof and a door: 6 flat colours.
fn illustration() -> (u32, u32, Vec<u8>) {
    let px = draw(320, 240, |x, y| {
        if (130.0..160.0).contains(&x) && (170.0..210.0).contains(&y) {
            [40, 40, 120] // door
        } else if (90.0..200.0).contains(&x) && (120.0..210.0).contains(&y) {
            [210, 60, 50] // wall
        } else if (60.0..120.0).contains(&y) && (x - 145.0).abs() <= (y - 60.0) * 1.1 {
            [90, 50, 20] // roof
        } else if inside_circle(x, y, 260.0, 50.0, 25.0) {
            [255, 210, 40] // sun
        } else if y >= 190.0 {
            [60, 160, 70] // ground
        } else {
            [150, 200, 240] // sky
        }
    });
    (320, 240, px)
}

/// Smooth gradients, noise and soft blobs, like a photo.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn photo(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 0x2545_f491_u32;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = f64::from(seed % 41) - 20.0;
            let (fx, fy) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
            let blob =
                |cx: f64, cy: f64, r: f64| (-((fx - cx).powi(2) + (fy - cy).powi(2)) / r).exp();
            let red = 90.0 + 120.0 * fx + 80.0 * blob(0.3, 0.4, 0.02);
            let green = 60.0 + 140.0 * fy + 60.0 * blob(0.7, 0.3, 0.05);
            let blue =
                140.0 + 60.0 * (fx * 9.0).sin() * (fy * 7.0).cos() + 90.0 * blob(0.5, 0.8, 0.03);
            for v in [red, green, blue] {
                out.push((v + noise).clamp(0.0, 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// A session holding the image (`w` × `h` RGBA), selected.
fn session((w, h, rgba): (u32, u32, Vec<u8>)) -> (Session, NodeId) {
    let png = rgba_png(w, h, rgba).expect("encodes");
    let imported = vextor_svg::import_image(&png).expect("png");
    let mut doc = vextor_svg::into_document(&imported, "trace").expect("places");
    // white, so what black and white leaves empty renders as the image does
    doc.artboards[0].background = Some(Rgba::rgb(255, 255, 255));
    let image = doc
        .nodes
        .values()
        .find(|n| matches!(n.kind, NodeKind::Image(_)))
        .expect("an image")
        .id;
    let mut s = Session::new(doc);
    s.select(vec![image]);
    (s, image)
}

/// Every mode's result for the selected image.
fn traced_modes(s: &Session) -> HashMap<TraceMode, Result<Vec<Shape>, String>> {
    let src = source(s).expect("traceable");
    let mut out = HashMap::new();
    trace_all(&src, TraceMode::BlackAndWhite, |mode, r| {
        out.insert(mode, r);
        true
    });
    out
}

fn shapes_of(s: &Session, mode: TraceMode) -> Vec<Shape> {
    traced_modes(s)
        .remove(&mode)
        .expect("every mode is traced")
        .expect("traces")
}

/// The document at 1:1, `w` × `h` from the origin (the image's place).
fn render_doc(doc: &Document, w: u32, h: u32) -> Pixmap {
    let viewport = Viewport {
        view: View {
            zoom: 1.0,
            pan: Vec2::new(0.0, 0.0),
        },
        width: w,
        height: h,
        pixels_per_point: 1.0,
    };
    render(doc, &Fonts::default(), &viewport).expect("valid viewport")
}

/// Share of pixels that differ by more than 10 % in a channel.
#[allow(clippy::cast_precision_loss)]
fn differing(a: &Pixmap, b: &Pixmap) -> f64 {
    let pixels = a.pixels().len();
    let off = a
        .pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(p, q)| {
            let (p, q) = (p.demultiply(), q.demultiply());
            p.red().abs_diff(q.red()) > 25
                || p.green().abs_diff(q.green()) > 25
                || p.blue().abs_diff(q.blue()) > 25
        })
        .count();
    off as f64 / pixels as f64
}

/// Renders the image, converts it (image removed), renders the result:
/// the share of pixels that differ.
fn render_difference(fixture: (u32, u32, Vec<u8>), mode: TraceMode) -> f64 {
    let (w, h) = (fixture.0, fixture.1);
    let (mut s, image) = session(fixture);
    let before = render_doc(&s.doc, w, h);
    let shapes = shapes_of(&s, mode);
    apply(&mut s, image, shapes, false).expect("applies");
    differing(&before, &render_doc(&s.doc, w, h))
}

/// Pixel tolerance of the render comparison on flat images.
const FLAT_TOLERANCE: f64 = 0.02;

#[test]
fn black_and_white_gives_one_shape_per_piece_with_its_holes() {
    // Owner review: one compound path per colour could not be taken apart;
    // each piece is its own shape.
    let (s, _) = session(logo());
    let shapes = shapes_of(&s, TraceMode::BlackAndWhite);
    assert_eq!(shapes.len(), 2, "the ring and the bar");
    let rules: Vec<vextor_core::FillRule> = shapes
        .iter()
        .map(|sh| match &sh.geometry {
            Geometry::Path(p) => p.fill_rule,
            _ => panic!("a path"),
        })
        .collect();
    assert!(
        rules.contains(&vextor_core::FillRule::EvenOdd),
        "the ring's hole"
    );
    assert!(rules.contains(&vextor_core::FillRule::NonZero), "the bar");
    for sh in &shapes {
        assert_eq!(sh.style.fill, Paint::Solid(Rgba::BLACK));
        assert_eq!(sh.style.stroke, None);
    }
}

#[test]
fn every_piece_can_be_moved_on_its_own_after_ungrouping() {
    let (mut s, image) = session(logo());
    let shapes = shapes_of(&s, TraceMode::BlackAndWhite);
    apply(&mut s, image, shapes, false).expect("applies");
    crate::edit_ops::ungroup(&mut s).expect("ungroups");
    let pieces = s.selected();
    assert_eq!(pieces.len(), 2, "two shapes, each selectable");
    let [ring, bar] = pieces[..] else {
        panic!("two pieces");
    };
    s.select(vec![bar]);
    let ring_before = s.doc.nodes[&ring].clone();
    crate::edit_ops::nudge(&mut s, 10.0, 0.0).expect("moves");
    assert_eq!(s.doc.nodes[&ring], ring_before, "the ring stays");
    assert_ne!(
        s.doc.nodes[&bar].transform, ring_before.transform,
        "the bar moved"
    );
}

#[test]
fn three_colours_draw_a_mid_tone_line_in_black() {
    // Owner review: with 3 slots the snout's mid-green ring joined the
    // body green and vanished; it is a drawn line, so it takes black.
    let (s, _) = session(cartoon());
    let shapes = shapes_of(&s, TraceMode::Colors3);
    let black: Vec<Shape> = shapes
        .into_iter()
        .filter(|sh| sh.style.fill == Paint::Solid(Rgba::BLACK))
        .collect();
    // on the ring, between the snout (r 63) and the head (r 70)
    assert!(distance_to(&black, kurbo::Point::new(263.0, 235.0)) < 1.5);
    assert!(distance_to(&black, kurbo::Point::new(270.0, 235.0)) < 1.5);
}

#[test]
fn three_colours_draw_darker_ear_spots_in_black() {
    // Owner review: the darker patches inside the ears vanished into the
    // body green; a darker spot enclosed by one area takes black.
    let (mut s, image) = session(cartoon());
    let shapes = shapes_of(&s, TraceMode::Colors3);
    apply(&mut s, image, shapes, false).expect("applies");
    let after = render_doc(&s.doc, 400, 380);
    let at = |x: u32, y: u32| {
        let c = after.pixel(x, y).expect("inside").demultiply();
        [c.red(), c.green(), c.blue()]
    };
    assert_eq!(at(120, 70), [0, 0, 0], "left ear spot");
    assert_eq!(at(280, 70), [0, 0, 0], "right ear spot");
    assert_eq!(
        at(120, 50),
        [0x6b, 0xe2, 0x48],
        "the ear around it stays green"
    );
}

#[test]
fn a_transparent_background_keeps_mid_tone_lines_in_black_and_white() {
    // Owner review: on a cut-out drawing the dark cut used to ignore the
    // transparent pixels, fell among the line colours and dropped the
    // snout's ring.
    let fixture = cartoon();
    let (mut s, image) = session(fixture);
    let shapes = shapes_of(&s, TraceMode::BlackAndWhite);
    apply(&mut s, image, shapes, false).expect("applies");
    let after = render_doc(&s.doc, 400, 380);
    let ink = |x: u32, y: u32| {
        let c = after.pixel(x, y).expect("inside").demultiply();
        c.red() < 64 && c.green() < 64 && c.blue() < 64
    };
    assert!(ink(266, 235), "the snout's ring (right side)");
    assert!(ink(200, 187), "the snout's ring (top)");
    assert!(ink(64 + 2, 215), "an eye's ring");
    assert!(ink(175, 235), "a nostril");
    assert!(!ink(200, 235), "the snout itself stays light");
    assert!(!ink(200, 320), "the head stays light");
}

#[test]
fn colour_modes_use_the_image_s_real_colours() {
    let (s, _) = session(cartoon());
    let colors: Vec<Paint> = shapes_of(&s, TraceMode::Colors3)
        .iter()
        .map(|sh| sh.style.fill)
        .collect();
    assert!(colors.contains(&Paint::Solid(Rgba::BLACK)), "{colors:?}");
    assert!(
        colors.contains(&Paint::Solid(Rgba::rgb(0x6b, 0xe2, 0x48))),
        "{colors:?}"
    );
}

#[test]
fn outlines_follow_every_edge_between_colours_once() {
    let (s, _) = session(cartoon());
    let lines = shapes_of(&s, TraceMode::Outlines);
    assert!(
        lines.len() > 1,
        "lines that meet are one piece, others apart"
    );
    for line in &lines {
        let stroke = line.style.stroke.as_ref().expect("stroked");
        assert_eq!(line.style.fill, Paint::None);
        assert!((stroke.width - 1.2).abs() < 1e-9, "0.3 % of 400 px");
        assert_eq!(stroke.paint, Paint::Solid(Rgba::BLACK));
    }
    // edges that are only a change of colour, missing before
    for (name, x, y) in [
        ("snout / its ring", 263.0, 235.0),
        ("its ring / head", 270.0, 235.0),
        ("ear spot", 134.0, 70.0),
        ("nostril", 185.0, 235.0),
        ("eye white / its ring", 69.0, 215.0),
    ] {
        let d = distance_to(&lines, kurbo::Point::new(x, y));
        assert!(d < 1.5, "{name}: {d:.2} px from the nearest line");
    }
    // the head's inside has no line
    assert!(distance_to(&lines, kurbo::Point::new(200.0, 320.0)) > 10.0);
}

#[test]
fn outlines_are_as_wide_as_a_share_of_the_image() {
    let small = Rect::new(0.0, 0.0, 100.0, 50.0);
    let big = Rect::new(0.0, 0.0, 2000.0, 1000.0);
    let width = |r| outline_style(r).stroke.expect("stroked").width;
    assert!((width(small) - 0.5).abs() < 1e-9, "never thinner than 0.5");
    assert!((width(big) - 6.0).abs() < 1e-9);
}

#[test]
fn colour_modes_give_at_most_3_or_6_colours() {
    let (s, _) = session(icon());
    assert!(colours(&shapes_of(&s, TraceMode::Colors3)).len() <= 3);
    let (s, _) = session(illustration());
    let six = shapes_of(&s, TraceMode::Colors6);
    let n = colours(&six).len();
    assert!((5..=6).contains(&n), "{n}");
    assert!(six.iter().all(|sh| sh.style.stroke.is_none()));
}

#[test]
fn flat_fixtures_render_like_the_image() {
    for (name, fixture, mode) in [
        ("logo", logo(), TraceMode::BlackAndWhite),
        ("icon", icon(), TraceMode::Colors3),
        ("illustration", illustration(), TraceMode::Colors6),
    ] {
        let off = render_difference(fixture, mode);
        assert!(
            off <= FLAT_TOLERANCE,
            "{name}: {:.2} % of pixels differ",
            off * 100.0
        );
    }
}

#[test]
fn the_same_image_and_mode_always_give_the_same_svg() {
    for (name, fixture, mode) in [
        ("logo_black_and_white", logo(), TraceMode::BlackAndWhite),
        ("logo_outlines", logo(), TraceMode::Outlines),
        ("icon_3_colours", icon(), TraceMode::Colors3),
        ("illustration_6_colours", illustration(), TraceMode::Colors6),
    ] {
        let (mut s, image) = session(fixture);
        let first = shapes_of(&s, mode);
        assert_eq!(shapes_of(&s, mode), first, "{name}: repeatable");
        apply(&mut s, image, first, false).expect("applies");
        let group = s.selected();
        let svg = vextor_svg::export(
            &s.doc,
            &Fonts::default(),
            Scope::Selection(&group),
            Format::default(),
            &s.doc.export,
        )
        .expect("exports")
        .code;
        assert!(!svg.contains("<image"), "{name}: only shapes");
        insta::assert_snapshot!(name, svg);
    }
}

#[test]
fn undo_restores_the_image_and_redo_gives_the_same_result() {
    let (mut s, image) = session(icon());
    let before = s.doc.clone();
    let shapes = shapes_of(&s, TraceMode::Colors3);
    apply(&mut s, image, shapes, false).expect("applies");
    let after = s.doc.clone();
    assert!(!after.nodes.contains_key(&image));
    assert!(s.undo());
    assert_eq!(s.doc, before);
    assert!(s.redo());
    assert_eq!(s.doc, after);
}

#[test]
fn keeping_the_image_hides_it_directly_under_the_group() {
    let (mut s, image) = session(logo());
    let shapes = shapes_of(&s, TraceMode::BlackAndWhite);
    apply(&mut s, image, shapes, true).expect("applies");
    let [group] = s.selected()[..] else {
        panic!("the group is selected");
    };
    let Some(Parent::Node(parent)) = s.doc.parent_of(group) else {
        panic!("in a layer");
    };
    let children = &s.doc.nodes[&parent].container().expect("a layer").children;
    let at = children.iter().position(|&c| c == group).expect("there");
    assert_eq!(children[at - 1], image);
    assert!(!s.doc.nodes[&image].visible);
    let g = &s.doc.nodes[&group];
    assert_eq!(g.name.as_deref(), Some(UNNAMED));
    assert_eq!(g.transform, s.doc.nodes[&image].transform);
    assert!(s.undo());
    assert!(s.doc.nodes[&image].visible);
    assert!(!s.doc.nodes.contains_key(&group));
}

#[test]
fn only_one_visible_unlocked_image_can_be_traced() {
    let (mut s, image) = session(logo());
    assert!(traceable(&s).is_ok());
    s.select(Vec::new());
    assert_eq!(traceable(&s).err(), Some(SELECT_ONE_IMAGE));
    s.select(vec![image]);
    let Some(Parent::Node(layer)) = s.doc.parent_of(image) else {
        panic!("in a layer");
    };
    let l = s.doc.nodes[&layer].clone();
    s.apply(Command::SetProps {
        id: layer,
        name: l.name,
        visible: true,
        locked: true,
        opacity: l.opacity,
    })
    .expect("locks");
    assert_eq!(traceable(&s).err(), Some(SELECT_ONE_IMAGE), "locked layer");
}

#[test]
fn an_image_that_does_not_decode_says_so() {
    let (mut s, image) = session(logo());
    if let Some(Node {
        kind: NodeKind::Image(img),
        ..
    }) = s.doc.nodes.get_mut(&image)
    {
        let ImageSource::Embedded { bytes, .. } = &mut img.source;
        *bytes = b"\x89PNG broken".to_vec();
    }
    let modes = traced_modes(&s);
    assert_eq!(modes.len(), MODES.len());
    assert!(
        modes
            .values()
            .all(|r| r.as_ref().err().map(String::as_str) == Some("Couldn't read this image"))
    );
}

#[test]
fn a_photo_in_6_colours_stays_under_the_limits_or_is_refused() {
    let (s, _) = session((640, 480, photo(640, 480)));
    match traced_modes(&s)
        .remove(&TraceMode::Colors6)
        .expect("traced")
    {
        Ok(shapes) => assert!(node_count(&shapes) <= vextor_geom::MAX_TRACE_NODES),
        Err(e) => assert_eq!(e, vextor_geom::TraceError::TooDetailed.to_string()),
    }
}
