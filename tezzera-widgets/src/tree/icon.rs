use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// Built-in icon shapes drawn with primitives (no image files needed).
#[derive(Debug, Clone, Copy)]
pub enum IconKind {
    Check,
    Close,
    Add,
    Remove,
    Search,
    Menu,
    Arrow,
    ChevronRight,
    ChevronDown,
    Settings,
    User,
    Home,
    Inbox,
    Calendar,
    Star,
    Heart,
    Bell,
    Edit,
    Trash,
    Upload,
    Download,
    Filter,
    Sort,
    Grid,
    List,
    Circle,
    Dot,
}

/// A vector icon drawn with canvas primitives at any size.
pub struct Icon {
    pub kind: IconKind,
    pub size: f32,
    pub color: Color,
}

impl Icon {
    pub fn new(kind: IconKind) -> Self {
        Self { kind, size: 16.0, color: Color::rgb(180, 184, 210) }
    }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
}

impl Widget for Icon {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.size, height: self.size }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let cx = r.origin.x + r.size.width / 2.0;
        let cy = r.origin.y + r.size.height / 2.0;
        let s = self.size;
        let c = self.color;

        match self.kind {
            IconKind::Check => {
                // ✓ check mark
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.35, y: cy }, size: Size { width: s*0.3, height: s*0.08 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.1, y: cy - s*0.2 }, size: Size { width: s*0.08, height: s*0.35 } }, c);
            }
            IconKind::Close | IconKind::Remove => {
                let h = s * 0.08;
                let half = s * 0.35;
                ctx.fill_rect(Rect { origin: Point { x: cx - half, y: cy - h/2.0 }, size: Size { width: half*2.0, height: h } }, c);
                if matches!(self.kind, IconKind::Close) {
                    ctx.fill_rect(Rect { origin: Point { x: cx - h/2.0, y: cy - half }, size: Size { width: h, height: half*2.0 } }, c);
                }
            }
            IconKind::Add => {
                let h = s * 0.08;
                let half = s * 0.35;
                ctx.fill_rect(Rect { origin: Point { x: cx - half, y: cy - h/2.0 }, size: Size { width: half*2.0, height: h } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - h/2.0, y: cy - half }, size: Size { width: h, height: half*2.0 } }, c);
            }
            IconKind::Search => {
                ctx.fill_circle(Point { x: cx - s*0.05, y: cy - s*0.05 }, s*0.28, c);
                ctx.fill_circle(Point { x: cx - s*0.05, y: cy - s*0.05 }, s*0.20, Color::rgba(0,0,0,0));
                ctx.fill_rect(Rect { origin: Point { x: cx + s*0.1, y: cy + s*0.1 }, size: Size { width: s*0.08, height: s*0.25 } }, c);
            }
            IconKind::Menu => {
                let h = s * 0.07;
                let w = s * 0.6;
                let lx = cx - w/2.0;
                ctx.fill_rect(Rect { origin: Point { x: lx, y: cy - s*0.22 }, size: Size { width: w, height: h } }, c);
                ctx.fill_rect(Rect { origin: Point { x: lx, y: cy - h/2.0 }, size: Size { width: w, height: h } }, c);
                ctx.fill_rect(Rect { origin: Point { x: lx, y: cy + s*0.15 }, size: Size { width: w, height: h } }, c);
            }
            IconKind::ChevronRight => {
                let t = s * 0.08;
                ctx.fill_rect(Rect { origin: Point { x: cx - t/2.0, y: cy - s*0.3 }, size: Size { width: t, height: s*0.3 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - t/2.0, y: cy }, size: Size { width: t, height: s*0.3 } }, c);
            }
            IconKind::ChevronDown => {
                let t = s * 0.08;
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.3, y: cy - t/2.0 }, size: Size { width: s*0.3, height: t } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx, y: cy - t/2.0 }, size: Size { width: s*0.3, height: t } }, c);
            }
            IconKind::Circle => {
                ctx.fill_circle(Point { x: cx, y: cy }, s * 0.4, c);
            }
            IconKind::Dot => {
                ctx.fill_circle(Point { x: cx, y: cy }, s * 0.2, c);
            }
            IconKind::User => {
                ctx.fill_circle(Point { x: cx, y: cy - s*0.12 }, s*0.22, c);
                ctx.fill_circle(Point { x: cx, y: cy + s*0.28 }, s*0.32, c);
            }
            IconKind::Home => {
                // Triangle roof + rect base
                let base_y = cy + s * 0.1;
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.28, y: base_y }, size: Size { width: s*0.56, height: s*0.3 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.08, y: base_y + s*0.05 }, size: Size { width: s*0.16, height: s*0.28 } }, Color::rgba(0,0,0,0));
                // Roof lines (simplified)
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.35, y: cy - s*0.05 }, size: Size { width: s*0.7, height: s*0.08 } }, c);
            }
            IconKind::Settings => {
                ctx.fill_circle(Point { x: cx, y: cy }, s*0.25, c);
                for i in 0..6 {
                    let angle = i as f32 * std::f32::consts::PI / 3.0;
                    let gx = cx + angle.cos() * s * 0.35;
                    let gy = cy + angle.sin() * s * 0.35;
                    ctx.fill_circle(Point { x: gx, y: gy }, s*0.08, c);
                }
            }
            IconKind::Bell => {
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.25, y: cy - s*0.1 }, size: Size { width: s*0.5, height: s*0.3 } }, c);
                ctx.fill_circle(Point { x: cx, y: cy - s*0.1 }, s*0.25, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.1, y: cy + s*0.2 }, size: Size { width: s*0.2, height: s*0.1 } }, c);
            }
            IconKind::Star => {
                ctx.fill_circle(Point { x: cx, y: cy }, s*0.3, c);
                for i in 0..5 {
                    let angle = i as f32 * std::f32::consts::PI * 2.0 / 5.0 - std::f32::consts::PI / 2.0;
                    let gx = cx + angle.cos() * s * 0.4;
                    let gy = cy + angle.sin() * s * 0.4;
                    ctx.fill_circle(Point { x: gx, y: gy }, s*0.07, c);
                }
            }
            IconKind::Inbox => {
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.3, y: cy - s*0.1 }, size: Size { width: s*0.6, height: s*0.35 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.3, y: cy - s*0.3 }, size: Size { width: s*0.08, height: s*0.25 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx + s*0.22, y: cy - s*0.3 }, size: Size { width: s*0.08, height: s*0.25 } }, c);
            }
            IconKind::Calendar => {
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.3, y: cy - s*0.2 }, size: Size { width: s*0.6, height: s*0.45 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.28, y: cy - s*0.05 }, size: Size { width: s*0.56, height: s*0.08 } }, Color::rgb(10,10,20));
            }
            IconKind::Edit => {
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.35, y: cy - s*0.05 }, size: Size { width: s*0.55, height: s*0.08 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx + s*0.15, y: cy - s*0.25 }, size: Size { width: s*0.08, height: s*0.3 } }, c);
            }
            IconKind::Trash => {
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.25, y: cy - s*0.1 }, size: Size { width: s*0.5, height: s*0.35 } }, c);
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.3, y: cy - s*0.2 }, size: Size { width: s*0.6, height: s*0.08 } }, c);
            }
            _ => {
                // Fallback: simple square
                ctx.fill_rect(Rect { origin: Point { x: cx - s*0.25, y: cy - s*0.25 }, size: Size { width: s*0.5, height: s*0.5 } }, c);
            }
        }
    }
}
