//! The font database: which families and faces exist, and which face a
//! [`FontSpec`] is drawn with (spec 05c).
//!
//! Fonts are read only from the OS font folders (or bytes a test hands
//! in), read-only; a malformed file is skipped by `fontdb`, and a face that
//! fails to parse later lays out like a missing font (`security-check` §2).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use vextor_core::FontSpec;

/// Families tried, in order, when a text's family is missing: a common
/// sans-serif of each platform, then the bundled test font.
const FALLBACK_FAMILIES: [&str; 6] = [
    "Helvetica",
    "Arial",
    "Segoe UI",
    "DejaVu Sans",
    "Liberation Sans",
    "Noto Sans",
];

/// The fonts text can use. Built once (system fonts load off the UI
/// thread, see [`FontDb::system`]) and then shared read-only.
#[derive(Default)]
pub struct FontDb {
    db: fontdb::Database,
    /// Family names, sorted case-insensitively, deduplicated.
    families: Vec<String>,
    /// Face bytes read so far, so a file is read once, not per layout.
    data: Mutex<BTreeMap<fontdb::ID, Arc<Vec<u8>>>>,
}

/// One face a family offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceStyle {
    pub weight: u16,
    pub italic: bool,
}

/// The face a [`FontSpec`] resolved to.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedFace {
    pub(crate) id: fontdb::ID,
    /// Family actually used (the requested one, or the fallback).
    pub family: String,
    pub style: FaceStyle,
    /// True when the requested family is not installed.
    pub missing: bool,
}

impl FontDb {
    /// No fonts at all; text lays out with estimated metrics and no glyphs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every font in the OS font folders. Slow (≈ 1 s on a Mac with 900
    /// faces): call it off the UI thread.
    #[must_use]
    pub fn system() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self::from_db(db)
    }

    /// The fonts in `fonts` (whole font files), for tests and bundles.
    #[must_use]
    pub fn from_data(fonts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut db = fontdb::Database::new();
        for data in fonts {
            db.load_font_data(data);
        }
        Self::from_db(db)
    }

    fn from_db(db: fontdb::Database) -> Self {
        let mut families: Vec<String> = db
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .filter(|name| !name.trim().is_empty())
            .collect();
        families.sort_by_key(|f| (f.to_lowercase(), f.clone()));
        families.dedup();
        Self {
            db,
            families,
            data: Mutex::default(),
        }
    }

    /// True when there is no face to draw with.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// Every family name, sorted case-insensitively.
    #[must_use]
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// The installed spelling of `family`, matched case-insensitively.
    #[must_use]
    pub fn family(&self, family: &str) -> Option<&str> {
        let want = family.trim().to_lowercase();
        self.families
            .iter()
            .find(|f| f.to_lowercase() == want)
            .map(String::as_str)
    }

    /// The faces `family` has (upright and italic weights), sorted; empty
    /// when the family is missing.
    #[must_use]
    pub fn styles(&self, family: &str) -> Vec<FaceStyle> {
        let Some(family) = self.family(family) else {
            return Vec::new();
        };
        let mut out: Vec<FaceStyle> = self
            .db
            .faces()
            .filter(|f| f.families.first().is_some_and(|(n, _)| n == family))
            .map(face_style)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The face `font` is drawn with: the nearest face of its family (CSS
    /// font matching), or of the first installed fallback family when the
    /// family is missing. `None` only when there are no fonts.
    #[must_use]
    pub fn resolve(&self, font: &FontSpec) -> Option<ResolvedFace> {
        let (family, missing) = match self.family(&font.family) {
            Some(f) => (f, false),
            None => (self.fallback_family()?, true),
        };
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(font.weight),
            stretch: fontdb::Stretch::Normal,
            style: if font.italic {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
        };
        let id = self.db.query(&query)?;
        let face = self.db.face(id)?;
        Some(ResolvedFace {
            id,
            family: family.to_owned(),
            style: face_style(face),
            missing,
        })
    }

    /// The family missing fonts are shown in.
    #[must_use]
    pub fn fallback_family(&self) -> Option<&str> {
        FALLBACK_FAMILIES
            .iter()
            .find_map(|f| self.family(f))
            .or_else(|| self.families.first().map(String::as_str))
    }

    /// The font file of `family`'s regular face (or its nearest) and the
    /// face's index in it, to show the family's name in its own face.
    #[must_use]
    pub fn sample_face(&self, family: &str) -> Option<(Arc<Vec<u8>>, u32)> {
        let face = self.resolve(&FontSpec {
            family: family.to_owned(),
            weight: 400,
            italic: false,
        })?;
        if face.missing {
            return None;
        }
        self.face_data(face.id)
    }

    /// The bytes of the file holding face `id`, and the face's index in it.
    pub(crate) fn face_data(&self, id: fontdb::ID) -> Option<(Arc<Vec<u8>>, u32)> {
        let index = self.db.face(id)?.index;
        let mut cache = self.data.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(data) = cache.get(&id) {
            return Some((Arc::clone(data), index));
        }
        let data = Arc::new(self.db.with_face_data(id, |data, _| data.to_vec())?);
        cache.insert(id, Arc::clone(&data));
        Some((data, index))
    }
}

fn face_style(f: &fontdb::FaceInfo) -> FaceStyle {
    FaceStyle {
        weight: f.weight.0,
        italic: f.style != fontdb::Style::Normal,
    }
}
