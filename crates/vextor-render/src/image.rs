//! Embedded images (spec 13): decoded once into premultiplied pixmaps and
//! kept in a small process-wide cache, so redrawing the canvas does not
//! decode them again.
//!
//! Image bytes come from files and the clipboard: decoding is bounded
//! (`image` limits on size and memory, `security-check` §2) and a broken
//! image draws nothing.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::sync::{Arc, Mutex, PoisonError};

use tiny_skia::{IntSize, Pixmap};
use vextor_core::ImageMime;

/// Images wider or taller than this are not decoded (the import limit).
const MAX_SIDE: u32 = 16_384;
/// Memory one decode may take.
const MAX_ALLOC: u64 = 1 << 30;
/// Decoded pixels kept at most (bytes); past it the cache starts over.
const CACHE_BYTES: usize = 512 << 20;
/// Byte locations remembered at most (each edit of an image adds one).
const MAX_PLACES: usize = 4096;

/// Where image bytes live, how long they are and a hash of their ends: a
/// cheap first look-up, redone after any edit or undo (the node, bytes
/// included, is cloned, so the same image moves).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Place {
    at: usize,
    len: usize,
    ends: u64,
}

impl Place {
    fn of(bytes: &[u8]) -> Self {
        const EDGE: usize = 4096;
        let mut h = DefaultHasher::new();
        bytes[..bytes.len().min(EDGE)].hash(&mut h);
        bytes[bytes.len().saturating_sub(EDGE)..].hash(&mut h);
        Self {
            at: bytes.as_ptr() as usize,
            len: bytes.len(),
            ends: h.finish(),
        }
    }
}

/// The bytes themselves (length and a hash of all of them): the same image
/// wherever it lives is decoded once.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Content {
    len: usize,
    hash: u64,
}

impl Content {
    fn of(bytes: &[u8]) -> Self {
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        Self {
            len: bytes.len(),
            hash: h.finish(),
        }
    }
}

#[derive(Default)]
struct Cache {
    places: HashMap<Place, Content>,
    images: HashMap<Content, Option<Arc<Pixmap>>>,
    bytes: usize,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// `bytes` (a `mime` image) as a pixmap; `None` when it cannot be decoded.
pub(crate) fn decoded(mime: ImageMime, bytes: &[u8]) -> Option<Arc<Pixmap>> {
    let place = Place::of(bytes);
    let lock = || CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(cache) = lock().as_ref()
        && let Some(content) = cache.places.get(&place)
        && let Some(hit) = cache.images.get(content)
    {
        return hit.clone();
    }
    let content = Content::of(bytes);
    if let Some(cache) = lock().as_mut()
        && let Some(hit) = cache.images.get(&content).cloned()
    {
        cache.places.insert(place, content);
        return hit;
    }
    // Decoded outside the lock: another thread may draw meanwhile.
    let pixmap = decode(mime, bytes).map(Arc::new);
    let mut guard = lock();
    let cache = guard.get_or_insert_with(Cache::default);
    let size = pixmap.as_ref().map_or(0, |p| p.data().len());
    if cache.bytes + size > CACHE_BYTES || cache.places.len() >= MAX_PLACES {
        *cache = Cache::default();
    }
    cache.bytes += size;
    cache.places.insert(place, content);
    cache.images.insert(content, pixmap.clone());
    pixmap
}

/// Straight (not premultiplied) RGBA pixels, `width` × `height`, as PNG
/// bytes: a pasted bitmap (spec 13) becomes an embedded PNG. `None` when the
/// pixel count does not match or a side is past the import limit.
#[must_use]
pub fn rgba_png(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let img = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn decode(mime: ImageMime, bytes: &[u8]) -> Option<Pixmap> {
    let format = match mime {
        ImageMime::Png => image::ImageFormat::Png,
        ImageMime::Jpeg => image::ImageFormat::Jpeg,
        ImageMime::Webp => image::ImageFormat::WebP,
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.into_rgba8();
    let size = IntSize::from_wh(rgba.width(), rgba.height())?;
    let mut data = rgba.into_raw();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in &mut px[..3] {
            // Rounded c × a / 255, always ≤ 255.
            #[allow(clippy::cast_possible_truncation)]
            let v = ((u16::from(*c) * a + 127) / 255) as u8;
            *c = v;
        }
    }
    Pixmap::from_vec(data, size)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// A photo-sized JPEG (3000 × 2000, smooth gradients).
    fn photo() -> Vec<u8> {
        let img = image::RgbImage::from_fn(3000, 2000, |x, y| {
            #[allow(clippy::cast_possible_truncation)] // x, y < 3000
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg)
            .expect("encodes");
        out
    }

    /// `cargo test --release -p vextor-render time_image_cache -- --ignored --nocapture`
    #[test]
    #[ignore = "timing, run by hand"]
    fn time_image_cache() {
        let bytes = photo();
        let t = Instant::now();
        assert!(decoded(ImageMime::Jpeg, &bytes).is_some());
        let miss = t.elapsed();
        let t = Instant::now();
        assert!(decoded(ImageMime::Jpeg, &bytes).is_some());
        let hit = t.elapsed();
        // An edit or undo clones the node: same bytes, new allocation.
        let copy = bytes.clone();
        let t = Instant::now();
        assert!(decoded(ImageMime::Jpeg, &copy).is_some());
        let moved = t.elapsed();
        println!(
            "{} KB jpeg: decode {miss:?}, hit {hit:?}, same bytes elsewhere {moved:?}",
            bytes.len() / 1024
        );
    }

    #[test]
    fn pasted_pixels_become_a_png_that_decodes_to_the_same_pixels() {
        let rgba = vec![255, 0, 0, 255, 0, 0, 255, 128];
        let png = rgba_png(2, 1, rgba).expect("encodes");
        let back = image::load_from_memory(&png).expect("decodes").into_rgba8();
        assert_eq!(back.into_raw(), [255, 0, 0, 255, 0, 0, 255, 128]);
        assert_eq!(rgba_png(2, 2, vec![0; 8]), None, "too few pixels");
        assert_eq!(
            rgba_png(MAX_SIDE + 1, 1, vec![0; 4 * (MAX_SIDE as usize + 1)]),
            None
        );
    }

    #[test]
    fn garbage_decodes_to_nothing_and_is_remembered() {
        assert!(decoded(ImageMime::Png, b"not a png").is_none());
        assert!(decoded(ImageMime::Jpeg, b"").is_none());
        assert!(decoded(ImageMime::Webp, b"RIFF\0\0\0\0WEBP").is_none());
    }
}
