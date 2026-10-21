//! View ▸ grid and snapping items, Object ▸ Add Artboard… and what they
//! do (spec 08).

use eframe::egui;

use super::keys::SHOW_GRID;
use super::{Action, VextorApp};
use crate::artboard;
use crate::new_doc::Picker;
use crate::snap_settings::{self, SnapOption};

impl VextorApp {
    /// View: Show Grid, the grid's size, the snapping toggles and Smart
    /// Guides (spec 08).
    pub(super) fn grid_items(
        &mut self,
        ui: &mut egui::Ui,
        has_doc: bool,
        actions: &mut Vec<Action>,
    ) {
        let grid = self
            .docs
            .session()
            .filter(|_| has_doc)
            .map(|s| s.doc.grid.clone());
        let on = |f: &dyn Fn(&vextor_core::GridSettings) -> bool| grid.as_ref().is_some_and(f);
        let show = egui::Button::new("Show Grid")
            .selected(on(&|g| g.visible))
            .shortcut_text(ui.ctx().format_shortcut(&SHOW_GRID));
        if ui.add_enabled(has_doc, show).clicked() {
            actions.push(Action::ToggleGrid);
        }
        ui.add_enabled_ui(has_doc, |ui| {
            ui.menu_button("Grid Size", |ui| self.grid_size(ui));
        });
        ui.separator();
        for option in SnapOption::ALL {
            let button = egui::Button::new(option.label()).selected(on(&|g| option.is_on(g)));
            if ui
                .add_enabled(has_doc, button)
                .on_hover_text(option.help())
                .clicked()
            {
                actions.push(Action::Snap(option));
            }
        }
        let mut guides = !self.canvas.hide_smart_guides;
        if ui
            .checkbox(&mut guides, "Smart Guides")
            .on_hover_text("Lines to the objects a moved selection lines up with")
            .changed()
        {
            self.canvas.hide_smart_guides = !guides;
        }
        ui.label(
            egui::RichText::new(format!(
                "Hold {} to snap freely",
                ui.ctx().format_modifiers(egui::Modifiers::COMMAND)
            ))
            .small()
            .weak(),
        );
    }

    /// View ▸ Grid Size: the major spacing and the minor lines per major.
    fn grid_size(&mut self, ui: &mut egui::Ui) {
        let Some(s) = self.docs.session_mut() else {
            return;
        };
        let (mut size, mut subdivisions) = (s.doc.grid.size, s.doc.grid.subdivisions);
        let mut changed = false;
        egui::Grid::new("grid-size").num_columns(2).show(ui, |ui| {
            ui.label("Size");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut size)
                        .range(0.1..=snap_settings::MAX_GRID)
                        .suffix(" px")
                        .max_decimals(2),
                )
                .changed();
            ui.end_row();
            ui.label("Subdivisions");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut subdivisions)
                        .range(1..=snap_settings::MAX_SUBDIVISIONS),
                )
                .on_hover_text("Minor lines per major line")
                .changed();
            ui.end_row();
        });
        if changed {
            snap_settings::set_size(s, size, subdivisions);
        }
    }

    /// The Add Artboard picker chose a size: `doc`'s artboard is added
    /// to the open document.
    pub(super) fn add_artboard(&mut self, doc: &vextor_core::Document) {
        self.picker = None;
        if let (Some(s), Some(a)) = (self.docs.session_mut(), doc.artboards.first()) {
            artboard::add(s, &a.name, a.rect.width, a.rect.height);
        }
    }

    /// Runs a grid, snapping or Add Artboard action; false for any other.
    pub(super) fn view_action(&mut self, action: &Action) -> bool {
        match *action {
            Action::AddArtboard => {
                if self.docs.session().is_some() {
                    let mut picker = Picker::default();
                    picker.adding = true;
                    self.picker = Some(picker);
                }
            }
            Action::ToggleGrid => {
                if let Some(s) = self.docs.session_mut() {
                    snap_settings::toggle_grid(s);
                }
            }
            Action::Snap(option) => {
                if let Some(s) = self.docs.session_mut() {
                    snap_settings::toggle(s, option);
                }
            }
            _ => return false,
        }
        true
    }
}
