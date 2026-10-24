//! Inspector ▸ Image (spec 13b): for one selected image, Image to
//! Vector…, like the Object menu's.

use eframe::egui;
use vextor_core::NodeKind;

use super::{PanelAction, section as panel_section};
use crate::session::Session;

/// The section, when one image is selected.
pub(super) fn section(ui: &mut egui::Ui, s: &Session, actions: &mut Vec<PanelAction>) {
    let [id] = s.selected()[..] else {
        return;
    };
    if !matches!(
        s.doc.nodes.get(&id).map(|n| &n.kind),
        Some(NodeKind::Image(_))
    ) {
        return;
    }
    panel_section(ui, "Image", |ui| {
        let traceable = crate::image_trace::traceable(s).is_ok();
        let button = ui
            .add_enabled(traceable, egui::Button::new("Image to Vector…"))
            .on_disabled_hover_text("Unlock and show the image and its layer first");
        if button.clicked() {
            actions.push(PanelAction::ImageToVector);
        }
    });
}
