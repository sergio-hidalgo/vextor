//! sRGB colour with alpha, edited as hex and saved as hex (default) or
//! as r/g/b/a numbers.

use std::cell::Cell;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ColorError;

/// sRGB, 8 bits per channel; `a` 0 = transparent, 255 = opaque. Parsed
/// from / formatted as `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
///
/// Saved as `"#1e90ff80"`, or as `{"r":30,"g":144,"b":255,"a":128}` while
/// a document whose [`ColorFormat`] is `Rgba` is being serialised. Either
/// form is accepted when loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    /// An opaque colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Parses `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`, case insensitive,
    /// with or without the leading `#`. Surrounding whitespace is ignored.
    ///
    /// # Errors
    /// [`ColorError`] when the length or a digit is wrong.
    pub fn from_hex(s: &str) -> Result<Self, ColorError> {
        let s = s.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        let digits = s
            .chars()
            .map(|c| {
                c.to_digit(16)
                    .and_then(|d| u8::try_from(d).ok())
                    .ok_or(ColorError::BadDigit(c))
            })
            .collect::<Result<Vec<u8>, _>>()?;
        let pair = |i: usize| digits[i] * 16 + digits[i + 1];
        match digits.len() {
            3 | 4 => {
                let a = digits.get(3).map_or(255, |d| d * 17);
                Ok(Self {
                    r: digits[0] * 17,
                    g: digits[1] * 17,
                    b: digits[2] * 17,
                    a,
                })
            }
            6 | 8 => {
                let a = if digits.len() == 8 { pair(6) } else { 255 };
                Ok(Self {
                    r: pair(0),
                    g: pair(2),
                    b: pair(4),
                    a,
                })
            }
            n => Err(ColorError::BadLength(n)),
        }
    }

    /// Lowercase `#rrggbb`, or `#rrggbbaa` when not fully opaque.
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// How colours are written when a document is saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    /// `"#1e90ff80"`.
    #[default]
    Hex,
    /// `{"r":30,"g":144,"b":255,"a":128}`.
    Rgba,
}

thread_local! {
    static SAVE_FORMAT: Cell<ColorFormat> = const { Cell::new(ColorFormat::Hex) };
}

/// While alive, colours serialised on this thread use `format`; the
/// previous format is restored on drop.
pub(crate) struct FormatScope(ColorFormat);

impl FormatScope {
    pub(crate) fn set(format: ColorFormat) -> Self {
        Self(SAVE_FORMAT.replace(format))
    }
}

impl Drop for FormatScope {
    fn drop(&mut self) {
        SAVE_FORMAT.set(self.0);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Channels {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Saved {
    Hex(String),
    Channels(Channels),
}

impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match SAVE_FORMAT.get() {
            ColorFormat::Hex => s.serialize_str(&self.to_hex()),
            ColorFormat::Rgba => Channels {
                r: self.r,
                g: self.g,
                b: self.b,
                a: self.a,
            }
            .serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Saved::deserialize(d)? {
            Saved::Hex(s) => Self::from_hex(&s).map_err(serde::de::Error::custom),
            Saved::Channels(Channels { r, g, b, a }) => Ok(Self { r, g, b, a }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
        Rgba { r, g, b, a }
    }

    #[test]
    fn parses_every_form() {
        let table = [
            ("#fff", rgba(255, 255, 255, 255)),
            ("FFF", rgba(255, 255, 255, 255)),
            ("#f0a8", rgba(255, 0, 170, 136)),
            ("#1E90ff", rgba(30, 144, 255, 255)),
            ("1e90FF80", rgba(30, 144, 255, 128)),
            ("  #000000  ", rgba(0, 0, 0, 255)),
            ("#00000000", rgba(0, 0, 0, 0)),
        ];
        for (input, want) in table {
            assert_eq!(Rgba::from_hex(input), Ok(want), "{input}");
        }
    }

    #[test]
    fn rejects_bad_input() {
        let table = [
            ("", ColorError::BadLength(0)),
            ("#", ColorError::BadLength(0)),
            ("#ff", ColorError::BadLength(2)),
            ("#fffff", ColorError::BadLength(5)),
            ("#fffffffff", ColorError::BadLength(9)),
            ("#ggg", ColorError::BadDigit('g')),
            ("##fff", ColorError::BadDigit('#')),
            ("#ff ff ff", ColorError::BadDigit(' ')),
            ("#ффф", ColorError::BadDigit('ф')),
        ];
        for (input, want) in table {
            assert_eq!(Rgba::from_hex(input), Err(want), "{input:?}");
        }
    }

    #[test]
    fn formats_short_when_opaque() {
        assert_eq!(rgba(30, 144, 255, 255).to_hex(), "#1e90ff");
        assert_eq!(rgba(30, 144, 255, 128).to_hex(), "#1e90ff80");
        assert_eq!(rgba(0, 0, 0, 0).to_hex(), "#00000000");
    }

    #[test]
    fn every_colour_round_trips_through_hex() {
        for v in (0..=255u8).step_by(5) {
            let c = rgba(v, 255 - v, v / 2, v);
            assert_eq!(Rgba::from_hex(&c.to_hex()), Ok(c));
        }
    }
}
