//! File ▸ Export… (spec 12): what to export (the current artboard, the
//! selection or every artboard), as SVG or PNG 1× / 2× / 3×, and for SVG
//! the export options. Export… then asks where (`app/export_flow.rs`).

use eframe::egui::{self, Rect};

use crate::code_panel::{Checks, options_ui};
use crate::export::{self, Kind, What};
use crate::prompts::{BUTTON_SIZE, buttons, centered, dialog_of_width};
use crate::session::Session;

/// The open dialog's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportDialog {
    pub what: What,
    pub kind: Kind,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            what: What::Artboard,
            kind: Kind::Svg,
        }
    }
}

/// The content width before the dialog has measured itself.
const FIRST_WIDTH: f32 = 340.0;

impl ExportDialog {
    /// Shows the dialog centred on `canvas`; options changes go straight
    /// to `s`. `Some(true)`: export with the choices; `Some(false)`:
    /// cancelled (also `Esc` or a click outside); `None` while open.
    ///
    /// Every row is centred, and the dialog is as wide as its widest row
    /// (measured last frame), so it narrows for PNG, which has no options.
    pub fn show(&mut self, ctx: &egui::Context, canvas: Rect, s: &mut Session) -> Option<bool> {
        let width_id = egui::Id::unique("export_dialog_width");
        let width = ctx
            .data(|d| d.get_temp::<f32>(width_id))
            .unwrap_or(FIRST_WIDTH);
        let modal = dialog_of_width(ctx, "export_dialog", canvas, width, |ui| {
            ui.add_space(4.0);
            ui.heading("Export");
            ui.add_space(10.0);
            let ((), what) = centered(ui, "export_what", |ui| self.what_row(ui, s));
            // A selection exports as SVG only.
            if export::why_not(s, self.what, self.kind).is_some()
                && export::why_not(s, self.what, Kind::Svg).is_none()
            {
                self.kind = Kind::Svg;
            }
            ui.add_space(6.0);
            let ((), kind) = centered(ui, "export_kind", |ui| self.kind_row(ui, s));
            let mut widest = what.max(kind);
            if self.kind == Kind::Svg {
                ui.add_space(10.0);
                let (changed, options) = centered(ui, "export_options", |ui| {
                    options_ui(ui, &s.doc.export, Checks::UnderFields)
                });
                if let Some(o) = changed {
                    s.set_export(o);
                }
                widest = widest.max(options);
            }
            let why = export::why_not(s, self.what, self.kind);
            if let Some(why) = why {
                ui.add_space(6.0);
                ui.colored_label(ui.visuals().warn_fg_color, why);
            }
            ui.add_space(12.0);
            let answer = buttons(ui, &[("Export…", true), ("Cancel", false)])
                .filter(|&go| !go || why.is_none());
            let buttons_row = BUTTON_SIZE.x * 2.0 + ui.spacing().item_spacing.x;
            (answer, widest.max(buttons_row))
        });
        let (answer, widest) = modal.inner;
        if (widest - width).abs() > 0.5 {
            ctx.data_mut(|d| d.insert_temp(width_id, widest));
            ctx.request_repaint();
        }
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        match answer {
            Some(answer) => Some(answer),
            None if enter && export::why_not(s, self.what, self.kind).is_none() => Some(true),
            None if modal.should_close() => Some(false),
            None => None,
        }
    }

    /// Artboard | Selection | All N artboards, disabled with the reason.
    fn what_row(&mut self, ui: &mut egui::Ui, s: &Session) {
        let artboards = s.doc.artboards.len();
        let all = if artboards == 1 {
            "All artboards".to_owned()
        } else {
            format!("All {artboards} artboards")
        };
        let choices = [
            (What::Artboard, "Artboard".to_owned()),
            (What::Selection, "Selection".to_owned()),
            (What::AllArtboards, all),
        ];
        for (what, title) in choices {
            let why = export::why_not(s, what, Kind::Svg);
            ui.add_enabled_ui(why.is_none(), |ui| {
                ui.selectable_value(&mut self.what, what, title);
            })
            .response
            .on_disabled_hover_text(why.unwrap_or_default());
        }
    }

    /// SVG | PNG 1× | PNG 2× | PNG 3×, disabled with the reason.
    fn kind_row(&mut self, ui: &mut egui::Ui, s: &Session) {
        for kind in Kind::ALL {
            let why = export::why_not(s, self.what, kind);
            ui.add_enabled_ui(why.is_none(), |ui| {
                ui.selectable_value(&mut self.kind, kind, kind.title());
            })
            .response
            .on_disabled_hover_text(why.unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    /// The dialog's content width after a few frames with `choice`.
    fn width(choice: ExportDialog) -> f32 {
        let mut s = Session::new(new_document("icon", 24.0, 24.0));
        let mut dialog = choice;
        let ctx = egui::Context::default();
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let canvas = ui.ctx().content_rect();
                let _ = dialog.show(ui.ctx(), canvas, &mut s);
            });
            out.textures_delta.clear();
        }
        ctx.data(|d| d.get_temp::<f32>(egui::Id::unique("export_dialog_width")))
            .expect("measured")
    }

    #[test]
    fn the_dialog_fits_its_rows_and_narrows_for_png() {
        let svg = width(ExportDialog {
            what: What::Artboard,
            kind: Kind::Svg,
        });
        let png = width(ExportDialog {
            what: What::Artboard,
            kind: Kind::Png(1),
        });
        assert!(png < svg, "PNG {png} vs SVG {svg}");
        assert!(png >= BUTTON_SIZE.x * 2.0, "{png}");
    }
}
