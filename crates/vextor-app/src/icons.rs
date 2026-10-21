//! Small line icons drawn with egui's painter (no icon font), for icon
//! buttons and field labels in the inspector. Every icon is designed on a
//! 16 × 16 grid and drawn at any square size.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Vec2};

use crate::theme;

mod path;
mod tools;

/// Side of the square an icon button's icon is drawn in, in points.
pub const SIZE: f32 = 16.0;
/// Side of a field-label icon, in points.
pub const LABEL_SIZE: f32 = 13.0;
/// Size of an icon button, in points.
const BUTTON: Vec2 = Vec2::new(28.0, 24.0);
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
}

/// A button showing `icon`, highlighted when `selected`, explained by
/// `tooltip` on hover, or by `why` on hover while disabled.
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    tooltip: &str,
    why: &str,
) -> Response {
    let r = ui
        .add_enabled(
            enabled,
            egui::Button::selectable(selected, "").min_size(BUTTON),
        )
        .on_hover_text(tooltip)
        .on_disabled_hover_text(why);
    let color = if !enabled {
        theme::GREY_500
    } else if selected {
        theme::YELLOW
    } else if r.hovered() {
        theme::WHITE
    } else {
        theme::GREY_100
    };
    let square = Rect::from_center_size(r.rect.center(), Vec2::splat(SIZE));
    paint(ui.painter(), square, icon, color);
    r
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
        _ => pen.path_icon(icon),
    }
}

/// Draws on the 16 × 16 grid at `origin`, scaled by `k`.
struct Pen<'a> {
    painter: &'a Painter,
    origin: Pos2,
    k: f32,
    stroke: Stroke,
    color: Color32,
}

type P = (f32, f32);

impl Pen<'_> {
    fn at(&self, (x, y): P) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.k
    }

    fn line(&self, a: P, b: P) {
        self.painter
            .line_segment([self.at(a), self.at(b)], self.stroke);
    }

    /// A line from `a` with an arrowhead at `b`.
    fn arrow(&self, a: P, b: P) {
        self.line(a, b);
        let (pa, pb) = (self.at(a), self.at(b));
        let dir = (pb - pa).normalized();
        let side = Vec2::new(-dir.y, dir.x);
        let head = 3.0 * self.k;
        for s in [1.0, -1.0] {
            self.painter
                .line_segment([pb, pb - dir * head + side * head * s * 0.8], self.stroke);
        }
    }

    /// An arc around `c` of radius `r` from angle `a0` to `a1` (radians,
    /// y down).
    fn arc(&self, c: P, r: f32, a0: f32, a1: f32) {
        let pts: Vec<Pos2> = (0..=8)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // eight steps
                let a = a0 + (a1 - a0) * i as f32 / 8.0;
                self.at((c.0 + r * a.cos(), c.1 + r * a.sin()))
            })
            .collect();
        self.painter.add(egui::Shape::line(pts, self.stroke));
    }

    fn rect(&self, a: P, b: P) -> Rect {
        Rect::from_two_pos(self.at(a), self.at(b))
    }

    fn fill(&self, a: P, b: P) {
        self.painter
            .rect_filled(self.rect(a, b), 1.0 * self.k, self.color);
    }

    fn outline(&self, a: P, b: P) {
        self.painter.rect_stroke(
            self.rect(a, b),
            1.0 * self.k,
            self.stroke,
            egui::StrokeKind::Middle,
        );
    }

    fn dashed_outline(&self, a: P, b: P) {
        let r = self.rect(a, b);
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        self.painter.extend(egui::Shape::dashed_line(
            &corners,
            Stroke::new(self.stroke.width * 0.8, self.color),
            2.0 * self.k,
            1.5 * self.k,
        ));
    }

    /// Four text lines, long and short in turn, lined up at `align`
    /// (0 left, 0.5 centre, 1 right).
    fn text_lines(&self, align: f32) {
        for (i, long) in [true, false, true, false].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // four lines
            let y = 3.0 + 3.5 * i as f32;
            let w = if long { 14.0 } else { 8.0 };
            let x0 = 1.0 + (14.0 - w) * align;
            self.line((x0, y), (x0 + w, y));
        }
    }

    /// A guide line and two bars lined up on it at `at` (0 start, 0.5
    /// centre, 1 end); `vertical` swaps the axes (top/middle/bottom).
    fn align(&self, vertical: bool, at: f32) {
        let t = |(x, y): P| if vertical { (y, x) } else { (x, y) };
        let guide = 1.5 + 13.0 * at;
        self.line(t((guide, 1.0)), t((guide, 15.0)));
        for (y0, y1, len) in [(3.0, 7.0, 11.0), (9.0, 13.0, 7.0)] {
            let x0 = if at < 0.25 {
                3.0
            } else if at > 0.75 {
                13.0 - len
            } else {
                8.0 - len / 2.0
            };
            self.fill(t((x0, y0)), t((x0 + len, y1)));
        }
    }

    /// Three bars evenly spaced; `vertical` stacks them.
    fn distribute(&self, vertical: bool) {
        let t = |(x, y): P| if vertical { (y, x) } else { (x, y) };
        for x in [1.0, 6.5, 12.0] {
            self.fill(t((x, 4.0)), t((x + 3.0, 12.0)));
        }
    }
}
