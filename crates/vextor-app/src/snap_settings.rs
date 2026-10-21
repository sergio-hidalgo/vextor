//! The View menu's grid and snap settings (spec 08): what each toggle
//! changes. They are view settings: saved with the document, not undone.

use vextor_core::GridSettings;

use crate::session::Session;

/// Largest grid size, in px.
pub const MAX_GRID: f64 = 1_000.0;
/// Most minor lines per major one.
pub const MAX_SUBDIVISIONS: u32 = 16;

/// A View ▸ snapping toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapOption {
    Grid,
    /// Nodes, edges and centres together.
    Objects,
    Artboard,
    Pixel,
}

impl SnapOption {
    /// In menu order.
    pub const ALL: [Self; 4] = [Self::Grid, Self::Objects, Self::Artboard, Self::Pixel];

    /// The menu item's label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Snap to Grid",
            Self::Objects => "Snap to Objects",
            Self::Artboard => "Snap to Artboard",
            Self::Pixel => "Pixel Snap",
        }
    }

    /// Its tooltip.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Grid => "Points and moved boxes land on grid lines",
            Self::Objects => "Snap to other objects' nodes, edges and centres",
            Self::Artboard => "Snap to the artboards' edges and centres",
            Self::Pixel => "Rectangles and lines land on whole px, for crisp icons",
        }
    }

    /// Whether it is on in `g`.
    #[must_use]
    pub fn is_on(self, g: &GridSettings) -> bool {
        let s = &g.snap;
        match self {
            Self::Grid => s.to_grid,
            Self::Objects => s.to_nodes || s.to_edges || s.to_centers,
            Self::Artboard => s.to_artboard,
            Self::Pixel => s.pixel,
        }
    }
}

/// Turns `option` on or off.
pub fn toggle(s: &mut Session, option: SnapOption) {
    let mut g = s.doc.grid.clone();
    let on = !option.is_on(&g);
    let snap = &mut g.snap;
    match option {
        SnapOption::Grid => snap.to_grid = on,
        SnapOption::Objects => {
            snap.to_nodes = on;
            snap.to_edges = on;
            snap.to_centers = on;
        }
        SnapOption::Artboard => snap.to_artboard = on,
        SnapOption::Pixel => snap.pixel = on,
    }
    s.set_grid(g);
}

/// Shows or hides the grid (`⌘'`).
pub fn toggle_grid(s: &mut Session) {
    let mut g = s.doc.grid.clone();
    g.visible = !g.visible;
    s.set_grid(g);
}

/// Sets the grid to `size` px with `subdivisions` minor lines per major
/// one, clamped to 0.1..=[`MAX_GRID`] and 1..=[`MAX_SUBDIVISIONS`]; the
/// grid shows. Returns false for a size that is not a number.
pub fn set_size(s: &mut Session, size: f64, subdivisions: u32) -> bool {
    if !size.is_finite() {
        return false;
    }
    let mut g = s.doc.grid.clone();
    g.size = size.clamp(0.1, MAX_GRID);
    g.subdivisions = subdivisions.clamp(1, MAX_SUBDIVISIONS);
    g.visible = true;
    s.set_grid(g);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use vextor_core::Document;

    fn session() -> Session {
        Session::new(Document::new("t"))
    }

    #[test]
    fn toggles_flip_their_settings_without_an_undo_step() {
        let mut s = session();
        assert!(!SnapOption::Grid.is_on(&s.doc.grid));
        toggle(&mut s, SnapOption::Grid);
        assert!(s.doc.grid.snap.to_grid);
        assert!(s.is_dirty(), "saved with the document");
        assert_eq!(s.edit_menu().undo, None, "a view setting is not undone");
        toggle(&mut s, SnapOption::Objects);
        let snap = &s.doc.grid.snap;
        assert!(!snap.to_nodes && !snap.to_edges && !snap.to_centers);
        toggle(&mut s, SnapOption::Pixel);
        assert!(s.doc.grid.snap.pixel);
    }

    #[test]
    fn show_grid_toggles_visibility() {
        let mut s = session();
        toggle_grid(&mut s);
        assert!(s.doc.grid.visible);
        toggle_grid(&mut s);
        assert!(!s.doc.grid.visible);
    }

    #[test]
    fn grid_size_is_clamped_and_shows_the_grid() {
        let mut s = session();
        assert!(set_size(&mut s, 0.0, 0));
        assert!((s.doc.grid.size - 0.1).abs() < 1e-12);
        assert_eq!(s.doc.grid.subdivisions, 1);
        assert!(s.doc.grid.visible);
        assert!(set_size(&mut s, 10.0, 5));
        assert!((s.doc.grid.size - 10.0).abs() < 1e-12);
        assert!(!set_size(&mut s, f64::NAN, 4));
    }
}
