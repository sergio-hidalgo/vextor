//! A headless canvas for tests: one egui context fed frame by frame (it
//! remembers the pointer between frames, so clicks and double-clicks
//! work), showing a session at 100 % with no pan in a 600×400 window.

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

use super::Canvas;
use crate::documents::{DocId, Documents};
use crate::new_doc::new_document;
use crate::session::Session;

pub(super) struct Harness {
    ctx: egui::Context,
    doc: DocId,
    pub(super) canvas: Canvas,
    /// Modifier keys held during the frames.
    pub(super) modifiers: Modifiers,
    /// Keep the session's view between frames (zoom tests) instead of
    /// putting it back at 100 % with no pan.
    pub(super) keep_view: bool,
}

impl Harness {
    pub(super) fn new() -> Self {
        let mut docs = Documents::default();
        let doc = docs.add(Session::new(new_document("x", 10.0, 10.0)), None);
        Self {
            ctx: egui::Context::default(),
            doc,
            canvas: Canvas::default(),
            modifiers: Modifiers::NONE,
            keep_view: false,
        }
    }

    /// One frame per event list.
    pub(super) fn run(&mut self, s: &mut Session, frames: Vec<Vec<Event>>) {
        for mut events in frames {
            if self.modifiers != Modifiers::NONE {
                events.insert(0, Event::ModifiersChanged(self.modifiers));
            }
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))),
                events,
                ..Default::default()
            };
            s.needs_fit = false;
            if !self.keep_view {
                s.editor.view = vextor_core::View::default();
            }
            let (canvas, doc) = (&mut self.canvas, self.doc);
            self.ctx
                .run_ui(input, |ui| {
                    egui::CentralPanel::no_frame().show(ui, |ui| {
                        let _ = canvas.ui(ui, doc, s);
                    });
                })
                .textures_delta
                .clear();
        }
    }
}

pub(super) fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

pub(super) fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// The frames of a click at `at`.
pub(super) fn click(at: Pos2) -> Vec<Vec<Event>> {
    vec![
        vec![Event::PointerMoved(at)],
        vec![button(at, true)],
        vec![button(at, false)],
        vec![],
    ]
}

/// The frames of a drag from `a` to `b` through their midpoint.
pub(super) fn drag(a: Pos2, b: Pos2) -> Vec<Vec<Event>> {
    vec![
        vec![Event::PointerMoved(a)],
        vec![button(a, true)],
        vec![Event::PointerMoved(a.lerp(b, 0.5))],
        vec![Event::PointerMoved(b)],
        vec![button(b, false)],
        vec![],
    ]
}
