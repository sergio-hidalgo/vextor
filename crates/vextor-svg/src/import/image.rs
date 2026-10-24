//! PNG, JPEG and WebP files as one embedded image node, at their pixel
//! size. The format and size come from the file's own header (never its
//! name), read before anything is decoded.

use vextor_core::{
    ClipboardPayload, Container, EPS, Image, ImageMime, ImageSource, MAX_FILE_BYTES, Node, NodeId,
    NodeKind, Rect, Transform,
};

use super::nodes::{Mapper, push};
use super::{Dropped, ImportError, Imported, MAX_IMAGE_SIDE, Report};

/// What a raster file's header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Raster {
    pub mime: ImageMime,
    pub width: u32,
    pub height: u32,
}

/// The format and size of image `bytes`.
pub(super) fn sniff(bytes: &[u8]) -> Result<Raster, ImportError> {
    let mime = match imagesize::image_type(bytes) {
        Ok(imagesize::ImageType::Png) => ImageMime::Png,
        Ok(imagesize::ImageType::Jpeg) => ImageMime::Jpeg,
        Ok(imagesize::ImageType::Webp) => ImageMime::Webp,
        _ => return Err(ImportError::UnsupportedImage),
    };
    let size = imagesize::blob_size(bytes).map_err(|_| ImportError::UnsupportedImage)?;
    let pixels = |v: usize| u32::try_from(v).ok().filter(|&v| v > 0);
    let (Some(width), Some(height)) = (pixels(size.width), pixels(size.height)) else {
        return Err(ImportError::UnsupportedImage);
    };
    if width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        return Err(ImportError::ImageTooLarge);
    }
    Ok(Raster {
        mime,
        width,
        height,
    })
}

/// A PNG, JPEG or WebP file as one image node, one image pixel per px,
/// its top-left at the origin.
///
/// # Errors
/// [`ImportError::TooLarge`] over the file size limit,
/// [`ImportError::UnsupportedImage`] for any other format or a broken
/// header, [`ImportError::ImageTooLarge`] past [`MAX_IMAGE_SIDE`].
pub fn import_image(bytes: &[u8]) -> Result<Imported, ImportError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    let Raster {
        mime,
        width,
        height,
    } = sniff(bytes)?;
    let (w, h) = (f64::from(width), f64::from(height));
    let node = Node::new(
        NodeId(1),
        NodeKind::Image(Image {
            rect: Rect::new(0.0, 0.0, w, h),
            source: ImageSource::Embedded {
                mime,
                bytes: bytes.to_vec(),
            },
        }),
    );
    Ok(Imported {
        payload: ClipboardPayload {
            nodes: vec![node],
            resources: Vec::new(),
        },
        size: (w, h),
        report: Report {
            objects: 1,
            ..Report::default()
        },
    })
}

impl Mapper<'_> {
    pub(super) fn image(
        &mut self,
        i: &usvg::Image,
        depth: usize,
        out: &mut Vec<Node>,
    ) -> Option<NodeId> {
        let (mime, bytes) = match i.kind() {
            usvg::ImageKind::PNG(d) => (ImageMime::Png, d),
            usvg::ImageKind::JPEG(d) => (ImageMime::Jpeg, d),
            usvg::ImageKind::WEBP(d) => (ImageMime::Webp, d),
            usvg::ImageKind::SVG(tree) => {
                // Only data usvg already accepted reaches here; an SVG
                // inside an image comes in as its objects.
                let mut ids = self.children(tree.root(), depth + 1, out);
                return match ids.len() {
                    0 => None,
                    1 => ids.pop(),
                    _ => {
                        let id = self.node_id();
                        let node = Node::new(
                            id,
                            NodeKind::Group(Container {
                                children: ids,
                                clip: None,
                            }),
                        );
                        Some(push(out, node))
                    }
                };
            }
            usvg::ImageKind::GIF(_) => {
                self.report.drop(Dropped::Image);
                return None;
            }
        };
        let size = i.size();
        let id = self.node_id();
        let mut node = Node::new(
            id,
            NodeKind::Image(Image {
                rect: Rect::new(0.0, 0.0, f64::from(size.width()), f64::from(size.height())),
                source: ImageSource::Embedded {
                    mime,
                    bytes: bytes.as_ref().clone(),
                },
            }),
        );
        node.name = self.name(i.id());
        node.visible = i.is_visible();
        Some(push(out, node))
    }
}

/// usvg sizes an image by its pixels and scales it with its group: the
/// scale goes into the image's rect (the model's image is its drawn
/// size), and a move too when nothing else is left.
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
pub(super) fn fit_image(image: &mut Image, t: &mut Transform) {
    let [a, b, c, d, e, f] = t.0;
    let sx = a.hypot(b);
    let det = a * d - b * c;
    // Only a rotation and a positive scale come apart this way.
    if sx <= EPS || det <= EPS || (a * c + b * d).abs() > 1e-6 * sx * sx.max(1.0) {
        return;
    }
    let sy = det / sx;
    let r = image.rect;
    image.rect = Rect::new(r.x * sx, r.y * sy, r.width * sx, r.height * sy);
    *t = Transform([a / sx, b / sx, c / sy, d / sy, e, f]);
    if crate::writer::is_translation(*t) {
        image.rect.x += e;
        image.rect.y += f;
        *t = Transform::IDENTITY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG header (signature + IHDR) claiming `w` × `h` pixels.
    pub(crate) fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        b.extend_from_slice(&13u32.to_be_bytes());
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn the_header_gives_format_and_size() {
        let r = sniff(&png_header(24, 16)).expect("png");
        assert_eq!((r.mime, r.width, r.height), (ImageMime::Png, 24, 16));
    }

    #[test]
    fn a_huge_or_unknown_image_is_refused() {
        assert_eq!(
            sniff(&png_header(MAX_IMAGE_SIDE + 1, 10)),
            Err(ImportError::ImageTooLarge)
        );
        assert_eq!(sniff(b"GIF89a...."), Err(ImportError::UnsupportedImage));
        assert_eq!(sniff(b""), Err(ImportError::UnsupportedImage));
        assert_eq!(
            sniff(&png_header(0, 10)),
            Err(ImportError::UnsupportedImage)
        );
    }
}
