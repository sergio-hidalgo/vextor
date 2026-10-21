//! The zoom bar (spec 08b): bottom-right of the canvas, `−`, the zoom as
//! a percentage, `+`. The percentage opens a menu of the zoom stops, Fit
//! Artboard and Actual Size, with a field to type a zoom.

use eframe::egui;

use super::Canvas;
use crate::session::Session;
use crate::view;

/// Space between the bar and the canvas's corner, in points.
const MARGIN: f32 = 8.0;
/// Width of the typed-zoom field, in points.
const FIELD_WIDTH: f32 = 64.0;

/// What the bar asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum BarAction {
    Step { inward: bool },
    To(f64),
    Fit,
    ActualSize,
}

impl Canvas {
    /// Draws the zoom bar over the canvas `rect` and applies what it asks.
    pub(super) fn zoom_bar(&mut self, ui: &egui::Ui, rect: egui::Rect, session: &mut Session) {
        let zoom = session.editor.view.zoom;
        let cmd = ui.ctx().format_modifiers(egui::Modifiers::COMMAND);
        let mut action = None;
        let mut menu_open = false;
        egui::Area::new(egui::Id::unique("zoom-bar"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(rect.right_bottom() - egui::vec2(MARGIN, MARGIN))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let out = ui
                                .add_enabled(!view::at_min(zoom), egui::Button::new("−"))
                                .on_hover_text(format!("Zoom out ({cmd}-)"))
                                .on_disabled_hover_text("Already at the smallest zoom, 10 %");
                            if out.clicked() {
                                action = Some(BarAction::Step { inward: false });
                            }
                            ui.menu_button(view::percent(zoom), |ui| {
                                menu_open = true;
                                if let Some(a) = self.zoom_menu(ui, zoom) {
                                    action = Some(a);
                                    ui.close();
                                }
                            })
                            .response
                            .on_hover_text("Choose or type a zoom");
                            let zin = ui
                                .add_enabled(!view::at_max(zoom), egui::Button::new("+"))
                                .on_hover_text(format!("Zoom in ({cmd}+)"))
                                .on_disabled_hover_text("Already at the largest zoom, 6 400 %");
                            if zin.clicked() {
                                action = Some(BarAction::Step { inward: true });
                            }
                        });
                    });
            });
        if !menu_open {
            self.zoom_text = None;
        }
        match action {
            Some(BarAction::Step { inward }) => self.zoom_step(session, inward),
            Some(BarAction::To(z)) => self.zoom_to(session, z),
            Some(BarAction::Fit) => self.fit(session),
            Some(BarAction::ActualSize) => self.actual_size(session),
            None => {}
        }
    }

    /// The percentage's menu: a field to type a zoom (Enter applies it),
    /// the stops, Fit Artboard and Actual Size.
    fn zoom_menu(&mut self, ui: &mut egui::Ui, zoom: f64) -> Option<BarAction> {
        let text = self
            .zoom_text
            .get_or_insert_with(|| format!("{:.0}", zoom * 100.0));
        let mut action = None;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(text)
                    .desired_width(FIELD_WIDTH)
                    .hint_text("100"),
            );
            ui.label("%");
            if r.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Some(z) = view::parse_percent(text)
            {
                action = Some(BarAction::To(z));
            }
        });
        ui.separator();
        for stop in view::ZOOM_STOPS {
            let on = (stop - zoom).abs() < stop * 1e-3;
            if ui
                .add(egui::Button::new(view::percent(stop)).selected(on))
                .clicked()
            {
                action = Some(BarAction::To(stop));
            }
        }
        ui.separator();
        if ui.button("Fit Artboard").clicked() {
            action = Some(BarAction::Fit);
        }
        if ui.button("Actual Size").clicked() {
            action = Some(BarAction::ActualSize);
        }
        action
    }
}
