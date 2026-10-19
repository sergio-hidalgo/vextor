//! Text objects: single-style point text with an optional frame (spec 05c).

use serde::{Deserialize, Serialize};

use crate::node::CornerRadii;
use crate::style::{Paint, Stroke, Style};

/// Most characters a text object may hold.
pub const MAX_TEXT_CHARS: usize = 10_000;

/// Longest font family name accepted, in characters.
pub const MAX_FONT_FAMILY_CHARS: usize = 256;

/// Lightest and boldest font weight (CSS `font-weight`).
pub const FONT_WEIGHTS: std::ops::RangeInclusive<u16> = 100..=900;

/// One line or more of text in one style. The node's transform places the
/// top-left of the first line's box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// Lines separated by `\n`; at most [`MAX_TEXT_CHARS`] characters.
    pub content: String,
    pub font: FontSpec,
    /// Font size in px, > 0.
    pub size: f64,
    /// Distance between baselines, as a multiple of `size`; > 0.
    pub line_height: f64,
    /// Extra space after each character, in px (may be negative).
    pub letter_spacing: f64,
    pub align: TextAlign,
    /// Glyph fill and stroke.
    pub style: Style,
    /// Background and border behind the text.
    pub frame: Option<TextFrame>,
}

/// Which face of which family. The family is kept even when the machine
/// does not have it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FontSpec {
    /// Not empty, at most [`MAX_FONT_FAMILY_CHARS`] characters.
    pub family: String,
    /// In [`FONT_WEIGHTS`]; 400 regular, 700 bold.
    pub weight: u16,
    pub italic: bool,
}

/// How lines line up with each other (SVG `text-anchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
    Right,
}

/// A label's background and border, drawn behind the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextFrame {
    /// Space around the text box: top, right, bottom, left (CSS order).
    pub padding: [f64; 4],
    pub radius: CornerRadii,
    pub fill: Paint,
    pub stroke: Option<Stroke>,
}

impl Text {
    /// `content` in `family` Regular, 16 px, line height 1.2, left
    /// aligned, black, no frame.
    #[must_use]
    pub fn new(content: impl Into<String>, family: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font: FontSpec {
                family: family.into(),
                weight: 400,
                italic: false,
            },
            size: 16.0,
            line_height: 1.2,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            style: Style::default(),
            frame: None,
        }
    }

    /// Every paint: glyph fill and stroke, then frame fill and border.
    pub fn paints(&self) -> impl Iterator<Item = Paint> + '_ {
        let frame = self
            .frame
            .iter()
            .flat_map(|f| std::iter::once(f.fill).chain(f.stroke.as_ref().map(|s| s.paint)));
        self.style.paints().chain(frame)
    }

    /// Mutable access to every paint, in [`Text::paints`] order.
    pub fn paints_mut(&mut self) -> Vec<&mut Paint> {
        let mut out = self.style.paints_mut();
        if let Some(f) = &mut self.frame {
            out.push(&mut f.fill);
            if let Some(s) = &mut f.stroke {
                out.push(&mut s.paint);
            }
        }
        out
    }
}
