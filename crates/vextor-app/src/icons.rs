//! Small line icons drawn with egui's painter (no icon font), for icon
//! buttons and field labels in the inspector. Every icon is designed on a
//! 16 × 16 grid and drawn at any square size.

use eframe::egui::{self, Color32, Painter, Rect, Response, Sense, Stroke, Vec2};

use crate::theme;

mod arrange;
mod buttons;
mod combine;
mod layers;
mod paint;
mod path;
mod pen;
mod tools;

pub use buttons::{button, choice_button, quiet_button, tight_button};
use pen::{P, Pen};

/// Side of the square an icon button's icon is drawn in, in points.
pub const SIZE: f32 = 16.0;
/// Side of a field-label icon, in points.
pub const LABEL_SIZE: f32 = 13.0;
/// Line width of an icon at 16 pt.
const LINE: f32 = 1.5;

/// The icons Vextor draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Text alignment: lines flush left / centred / flush right.
    TextLeft,
    TextCentre,
    TextRight,
    /// Field labels: X (arrow to an edge), Y, W (|↔|), H, rotation angle.
    PosX,
    PosY,
    Width,
    Height,
    Angle,
    /// Align objects: left / centre / right edges, top / middle / bottom.
    AlignLeft,
    AlignCentre,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    /// Distribute objects evenly side by side / one above the other.
    DistributeH,
    DistributeV,
    /// Align to the artboard (a board with an object in it).
    Artboard,
    Group,
    Ungroup,
    /// Node types (spec 07): a sharp corner; a smooth node (handles in
    /// line, lengths free); a symmetric node (handles in line, equal).
    NodeCorner,
    NodeSmooth,
    NodeSymmetric,
    /// A node's incoming (●—) and outgoing (—●) handle, for labels.
    HandleIn,
    HandleOut,
    /// Path commands: close, open at a node, join two ends, split at a
    /// node (scissors), reverse direction.
    PathClose,
    PathOpen,
    PathJoin,
    PathSplit,
    PathReverse,
    /// Path operations (spec 11): two overlapping circles, the result
    /// filled (both; the back one less the front; the overlap; all but
    /// the overlap).
    Union,
    Subtract,
    Intersect,
    Exclude,
    /// The tools strip (spec 07): Select (a solid arrow), Nodes (a hollow
    /// arrow with a node), Pen (a nib), Text, and the shapes.
    ToolSelect,
    ToolNode,
    ToolPen,
    ToolText,
    ToolRect,
    ToolEllipse,
    ToolPolygon,
    ToolStar,
    ToolLine,
    /// The zoom tool (spec 08b): a magnifier.
    ToolZoom,
    /// Artboards list (spec 08): add (a plus), move earlier / later in
    /// the order (arrows), delete (a bin).
    Add,
    MoveUp,
    MoveDown,
    Delete,
    /// Arrange (spec 08c): stacked squares with an arrow up or down.
    ArrangeFront,
    ArrangeForward,
    ArrangeBackward,
    ArrangeBack,
    /// Paint (spec 09): a pipette that picks a colour from the canvas.
    Eyedropper,
    /// Rect corner radius: a corner bracket with a dot inside that corner;
    /// all four small for every corner at once.
    CornerTopLeft,
    CornerTopRight,
    CornerBottomRight,
    CornerBottomLeft,
    CornerAll,
    /// Flip: two outlined triangles mirrored across a narrow gap.
    FlipHorizontal,
    FlipVertical,
    /// Stroke caps, drawn like the Align icons: a filled bar (the stroke)
    /// and a guide line where the path ends; the bar stops at it (butt),
    /// ends past it in a half disc (round) or square (projecting).
    CapButt,
    CapRound,
    CapSquare,
    /// Stroke joins: a filled L-shaped corner, its outer corner sharp
    /// (miter), rounded or cut (bevel).
    JoinMiter,
    JoinRound,
    JoinBevel,
    /// Layers panel (spec 10): visible / hidden, locked / unlocked, the
    /// row icons of a layer and an image, and Duplicate layer.
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Layer,
    Image,
    Duplicate,
}

/// Size of a tool button in the tools strip, and of its icon, in points.
const TOOL_BUTTON: Vec2 = Vec2::new(34.0, 32.0);
const TOOL_ICON: f32 = 20.0;

/// A tools strip button: `icon` large, highlighted while `selected`
/// (the active tool), explained by `tooltip`.
pub fn tool_button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let r = ui
        .add_enabled(
            enabled,
            egui::Button::selectable(selected, "").min_size(TOOL_BUTTON),
        )
        .on_hover_text(tooltip);
    let color = if !enabled {
        theme::GREY_500
    } else if selected {
        theme::YELLOW
    } else if r.hovered() {
        theme::WHITE
    } else {
        theme::GREY_100
    };
    let square = Rect::from_center_size(r.rect.center(), Vec2::splat(TOOL_ICON));
    paint(ui.painter(), square, icon, color);
    r
}

/// A small grey icon after a field's label, explained by `tooltip`.
pub fn label(ui: &mut egui::Ui, icon: Icon, tooltip: &str) {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(LABEL_SIZE), Sense::hover());
    paint(ui.painter(), rect, icon, theme::GREY_400);
    r.on_hover_text(tooltip);
}

/// Draws `icon` into the square `rect` in `color`.
#[allow(clippy::too_many_lines)] // one short drawing per icon
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let k = rect.width() / 16.0;
    let pen = Pen {
        painter,
        origin: rect.min,
        k,
        stroke: Stroke::new(LINE * k.max(0.75), color),
        color,
    };
    match icon {
        Icon::TextLeft => pen.text_lines(0.0),
        Icon::TextCentre => pen.text_lines(0.5),
        Icon::TextRight => pen.text_lines(1.0),
        Icon::PosX => {
            pen.line((14.0, 3.0), (14.0, 13.0));
            pen.arrow((2.0, 8.0), (12.0, 8.0));
        }
        Icon::PosY => {
            pen.line((3.0, 14.0), (13.0, 14.0));
            pen.arrow((8.0, 2.0), (8.0, 12.0));
        }
        Icon::Width => {
            pen.line((1.5, 4.0), (1.5, 12.0));
            pen.line((14.5, 4.0), (14.5, 12.0));
            pen.arrow((8.0, 8.0), (3.5, 8.0));
            pen.arrow((8.0, 8.0), (12.5, 8.0));
        }
        Icon::Height => {
            pen.line((4.0, 1.5), (12.0, 1.5));
            pen.line((4.0, 14.5), (12.0, 14.5));
            pen.arrow((8.0, 8.0), (8.0, 3.5));
            pen.arrow((8.0, 8.0), (8.0, 12.5));
        }
        Icon::Angle => {
            pen.line((2.0, 13.0), (14.0, 13.0));
            pen.line((2.0, 13.0), (11.0, 3.0));
            pen.arc((2.0, 13.0), 6.5, -0.84, 0.0);
        }
        Icon::AlignLeft => pen.align(false, 0.0),
        Icon::AlignCentre => pen.align(false, 0.5),
        Icon::AlignRight => pen.align(false, 1.0),
        Icon::AlignTop => pen.align(true, 0.0),
        Icon::AlignMiddle => pen.align(true, 0.5),
        Icon::AlignBottom => pen.align(true, 1.0),
        Icon::DistributeH => pen.distribute(false),
        Icon::DistributeV => pen.distribute(true),
        Icon::Artboard => {
            pen.outline((1.5, 2.5), (14.5, 13.5));
            pen.fill((5.5, 5.5), (10.5, 10.5));
        }
        Icon::Group => {
            pen.dashed_outline((1.0, 1.0), (15.0, 15.0));
            pen.fill((4.0, 4.0), (9.0, 9.0));
            pen.fill((8.0, 8.0), (12.0, 12.0));
        }
        Icon::Ungroup => {
            pen.outline((1.5, 1.5), (7.5, 7.5));
            pen.outline((8.5, 8.5), (14.5, 14.5));
        }
        Icon::Add => {
            pen.line((8.0, 2.5), (8.0, 13.5));
            pen.line((2.5, 8.0), (13.5, 8.0));
        }
        Icon::MoveUp => pen.arrow((8.0, 14.0), (8.0, 2.5)),
        Icon::MoveDown => pen.arrow((8.0, 2.0), (8.0, 13.5)),
        Icon::Delete => {
            pen.line((2.0, 4.0), (14.0, 4.0));
            pen.line((6.0, 4.0), (6.5, 1.5));
            pen.line((6.5, 1.5), (9.5, 1.5));
            pen.line((9.5, 1.5), (10.0, 4.0));
            pen.line((3.5, 4.0), (4.5, 14.5));
            pen.line((4.5, 14.5), (11.5, 14.5));
            pen.line((11.5, 14.5), (12.5, 4.0));
            pen.line((6.5, 7.0), (6.5, 12.0));
            pen.line((9.5, 7.0), (9.5, 12.0));
        }
        Icon::Eyedropper
        | Icon::CornerTopLeft
        | Icon::CornerTopRight
        | Icon::CornerBottomRight
        | Icon::CornerBottomLeft
        | Icon::CornerAll
        | Icon::FlipHorizontal
        | Icon::FlipVertical
        | Icon::CapButt
        | Icon::CapRound
        | Icon::CapSquare
        | Icon::JoinMiter
        | Icon::JoinRound
        | Icon::JoinBevel => pen.paint_icon(icon),
        Icon::ToolSelect
        | Icon::ToolNode
        | Icon::ToolPen
        | Icon::ToolText
        | Icon::ToolRect
        | Icon::ToolEllipse
        | Icon::ToolPolygon
        | Icon::ToolStar
        | Icon::ToolLine
        | Icon::ToolZoom => pen.tool_icon(icon),
        Icon::ArrangeFront | Icon::ArrangeForward | Icon::ArrangeBackward | Icon::ArrangeBack => {
            pen.arrange_icon(icon);
        }
        Icon::Eye
        | Icon::EyeOff
        | Icon::Lock
        | Icon::Unlock
        | Icon::Layer
        | Icon::Image
        | Icon::Duplicate => pen.layers_icon(icon),
        Icon::Union | Icon::Subtract | Icon::Intersect | Icon::Exclude => {
            pen.combine_icon(icon);
        }
        _ => pen.path_icon(icon),
    }
}
