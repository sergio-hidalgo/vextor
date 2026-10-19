//! What a Text section change does to each selected text (spec 05c).

use vextor_core::{Command, CornerRadii, NodeId, Paint, Rgba, Stroke, Text, TextAlign, TextFrame};
use vextor_geom::FaceStyle;

/// The frame a text gets when a background or border is first turned on.
pub(super) fn empty_frame() -> TextFrame {
    TextFrame {
        padding: [8.0, 16.0, 8.0, 16.0],
        radius: CornerRadii {
            tl: 6.0,
            tr: 6.0,
            br: 6.0,
            bl: 6.0,
        },
        fill: Paint::None,
        stroke: None,
    }
}

/// One change from the Text section.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Edit {
    Family(String),
    Style(FaceStyle),
    Size(f64),
    LineHeight(f64),
    Spacing(f64),
    Align(TextAlign),
    Color(Rgba),
    /// Background colour, or `None` to remove it.
    Background(Option<Rgba>),
    PaddingX(f64),
    PaddingY(f64),
    Radius(f64),
    /// Border, or `None` to remove it.
    Border(Option<Stroke>),
    BorderWidth(f64),
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
            Self::Color(c) => t.style.fill = Paint::Solid(*c),
            Self::Background(c) => {
                let mut f = frame();
                f.fill = c.map_or(Paint::None, Paint::Solid);
                t.frame = Some(f);
            }
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
            Self::Border(b) => {
                let mut f = frame();
                f.stroke.clone_from(b);
                t.frame = Some(f);
            }
            Self::BorderWidth(w) => {
                let mut f = frame();
                if let Some(b) = &mut f.stroke {
                    b.width = *w;
                }
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
            Self::Color(_) => "Change text colour",
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
