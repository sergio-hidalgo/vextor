//! The side and bottom panels. Placeholders until their specs fill them.

use eframe::egui;

/// Tools (left). Only selection exists until the drawing tools arrive.
pub fn tools(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        ui.add(egui::Button::selectable(true, "V").min_size(egui::vec2(32.0, 32.0)))
            .on_hover_text("Select (V)");
    });
}

/// Inspector and Layers (right).
pub fn inspector_and_layers(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.strong("Inspector");
    ui.weak("Nothing selected.");
    ui.add_space(12.0);
    ui.separator();
    ui.strong("Layers");
    ui.weak("The layer list will appear here.");
}

/// Code (bottom).
pub fn code(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.strong("Code");
    ui.weak("The SVG of the artboard will appear here.");
}
