//! What a Text section change does to each selected text (spec 05c).
//! Colours, background and border are paints (spec 09, `paint_edit`).

use vextor_core::{Command, CornerRadii, NodeId, Paint, Text, TextAlign};
use vextor_geom::FaceStyle;

use crate::paint_edit::empty_frame;

/// One change from the Text section.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Edit {
    Family(String),
    Style(FaceStyle),
    Size(f64),
    LineHeight(f64),
    Spacing(f64),
    Align(TextAlign),
    PaddingX(f64),
    PaddingY(f64),
    Radius(f64),
}

impl Edit {
    fn apply(&self, t: &mut Text) {
        let frame = || t.frame.clone().unwrap_or_else(empty_frame);
        match self {
            Self::Family(f) => t.font.family.clone_from(f),
            Self::Style(s) => {
                t.font.weight = s.weight;
                t.font.italic = s.italic;
            }
            Self::Size(v) => t.size = *v,
            Self::LineHeight(v) => t.line_height = *v,
            Self::Spacing(v) => t.letter_spacing = *v,
            Self::Align(a) => t.align = *a,
            Self::PaddingX(v) => {
                let mut f = frame();
                f.padding[1] = *v;
                f.padding[3] = *v;
                t.frame = Some(f);
            }
            Self::PaddingY(v) => {
                let mut f = frame();
                f.padding[0] = *v;
                f.padding[2] = *v;
                t.frame = Some(f);
            }
            Self::Radius(v) => {
                let mut f = frame();
                f.radius = CornerRadii {
                    tl: *v,
                    tr: *v,
                    br: *v,
                    bl: *v,
                };
                t.frame = Some(f);
            }
        }
        // Neither background nor border: no frame at all.
        if t.frame
            .as_ref()
            .is_some_and(|f| f.fill == Paint::None && f.stroke.is_none())
        {
            t.frame = None;
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Family(_) | Self::Style(_) => "Change font",
            Self::Size(_) | Self::LineHeight(_) | Self::Spacing(_) | Self::Align(_) => {
                "Change text style"
            }
            _ => "Change text frame",
        }
    }
}

/// One undo step applying `edit` to every text, each keeping its other
/// values.
pub(super) fn restyle(texts: &[(NodeId, Text)], edit: &Edit) -> Command {
    let mut commands = Vec::new();
    for (id, before) in texts {
        let mut t = before.clone();
        edit.apply(&mut t);
        let ids = vec![*id];
        if t.style != before.style {
            commands.push(Command::SetStyle {
                ids: ids.clone(),
                style: t.style.clone(),
            });
        }
        if t.frame != before.frame {
            commands.push(Command::SetTextFrame {
                ids: ids.clone(),
                frame: t.frame.clone(),
            });
        }
        commands.push(Command::SetTextStyle {
            ids,
            font: t.font,
            size: t.size,
            line_height: t.line_height,
            letter_spacing: t.letter_spacing,
            align: t.align,
        });
    }
    Command::Batch {
        label: edit.label(),
        commands,
    }
}
