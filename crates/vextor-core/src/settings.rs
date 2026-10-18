//! Per-document settings saved with the file: grid, snapping, SVG export.

use serde::{Deserialize, Serialize};

/// The canvas grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridSettings {
    /// Px between major lines, > 0. Default 8.
    pub size: f64,
    /// Minor lines per major, ≥ 1. Default 4 (→ 2 px).
    pub subdivisions: u32,
    pub visible: bool,
    pub snap: SnapSettings,
}

/// What pointer positions snap to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent toggles, one per checkbox
pub struct SnapSettings {
    pub to_grid: bool,
    pub to_nodes: bool,
    pub to_edges: bool,
    pub to_centers: bool,
    pub to_artboard: bool,
    /// Shift-constrain step, > 0. Default 45.
    pub angle_step_deg: f64,
    /// Screen px, ≥ 0. Default 6.
    pub tolerance_px: f64,
}

/// Options for the emitted SVG (spec 12).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // independent toggles, one per checkbox
pub struct SvgExportOptions {
    /// Decimal places, 0..=6. Default 2.
    pub precision: u8,
    pub styling: Styling,
    pub ids: IdMode,
    /// `viewBox` only, no `width`/`height`.
    pub responsive: bool,
    pub minify: bool,
    /// Map the dominant solid colour to `currentColor`.
    pub current_color: bool,
}

/// Where paint is written in the SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Styling {
    /// `fill="…"` presentation attributes.
    Attributes,
    /// One `<style>` element with classes.
    StyleElement,
}

/// How element ids are emitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdMode {
    /// No ids except those `<defs>` references need.
    None,
    /// Node names as ids.
    Readable,
    /// Names prefixed to stay unique when several SVGs share a page.
    UniquePrefixed,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            size: 8.0,
            subdivisions: 4,
            visible: false,
            snap: SnapSettings::default(),
        }
    }
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            to_grid: false,
            to_nodes: true,
            to_edges: true,
            to_centers: true,
            to_artboard: true,
            angle_step_deg: 45.0,
            tolerance_px: 6.0,
        }
    }
}

impl Default for SvgExportOptions {
    fn default() -> Self {
        Self {
            precision: 2,
            styling: Styling::Attributes,
            ids: IdMode::Readable,
            responsive: false,
            minify: false,
            current_color: false,
        }
    }
}
