use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::{Color, DrawCommand};
use tezzera_theme::TitleAlign;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A top app bar with title, leading, and trailing action slots.
///
/// Platform-adaptive (D105 Phase 23): `height`, `show_traffic_lights`, and
/// title alignment default to the active theme's `app_bar` style
/// (`ThemeData::app_bar`, an [`tezzera_theme::AppBarStyle`]) — the SAME
/// widget renders macOS/iOS/Android-appropriate chrome purely from theme
/// data, no platform branch in this file. Per-instance builder calls
/// (`.height(..)`, `.traffic_lights()`) override the theme for that one
/// instance; a widget that doesn't call them follows the theme.
pub struct AppBar {
    pub title: String,
    pub title_size: f32,
    pub background: Color,
    pub foreground: Color,
    pub border_color: Color,
    /// `None` = use the active theme's `app_bar.height`.
    height: Option<f32>,
    pub leading: Option<BoxedWidget>,
    pub actions: Vec<BoxedWidget>,
    /// `None` = use the active theme's `app_bar.show_traffic_lights`.
    show_traffic_lights: Option<bool>,
}

impl AppBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            title_size: 13.0,
            background: Color::rgba(0, 0, 0, 0), // sentinel: use theme.surface
            foreground: Color::rgba(0, 0, 0, 0), // sentinel: use theme.on_surface
            border_color: Color::rgba(0, 0, 0, 0), // sentinel: use theme.outline
            height: None,
            leading: None,
            actions: Vec::new(),
            show_traffic_lights: None,
        }
    }

    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn foreground(mut self, c: Color) -> Self { self.foreground = c; self }
    /// Overrides the active theme's app-bar height for this instance.
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn action(mut self, w: impl Widget + 'static) -> Self { self.actions.push(Box::new(w)); self }
    /// Overrides the active theme's traffic-light setting for this instance.
    pub fn no_traffic_lights(mut self) -> Self { self.show_traffic_lights = Some(false); self }
    /// Draw faux macOS traffic-light dots (only for standalone mockup
    /// screenshots — a real app window already has real OS traffic lights).
    /// Overrides the active theme's traffic-light setting for this instance.
    pub fn traffic_lights(mut self) -> Self { self.show_traffic_lights = Some(true); self }
    pub fn title_size(mut self, s: f32) -> Self { self.title_size = s; self }

    fn effective_height(&self, theme: &tezzera_theme::ThemeData) -> f32 {
        self.height.unwrap_or(theme.app_bar.height)
    }
}

impl Widget for AppBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: self.effective_height(ctx.theme) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let style = ctx.theme.app_bar;
        let t = &ctx.theme.colors;
        let bg     = if self.background.a   == 0 { ctx.tc(t.surface)     } else { self.background   };
        let fg     = if self.foreground.a   == 0 { ctx.tc(t.on_surface)  } else { self.foreground   };
        let border = if self.border_color.a == 0 { ctx.tc(t.outline)     } else { self.border_color };
        let show_traffic_lights = self.show_traffic_lights.unwrap_or(style.show_traffic_lights);

        let r = ctx.rect;
        ctx.fill_rect(r, bg);

        // Separating edge — theme-controlled (elevation > 0 draws it; 0 omits
        // it entirely, e.g. the flat Cupertino look).
        if style.elevation > 0.0 {
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - 1.0 },
                size: Size { width: r.size.width, height: 1.0 },
            }, border);
        }

        let cy = r.origin.y + r.size.height / 2.0;
        let mut lx = r.origin.x + 16.0;

        // Traffic lights (opt-in mockup chrome).
        if show_traffic_lights {
            for (i, color) in [
                Color::rgb(235, 85, 75),
                Color::rgb(245, 185, 55),
                Color::rgb(75, 200, 85),
            ].iter().enumerate() {
                ctx.fill_circle(Point { x: lx + i as f32 * 20.0, y: cy }, 7.0, *color);
            }
            lx += 72.0;
        }

        // Leading widget — sized to its content (up to a sane cap), advancing
        // the left boundary so the title never overlaps it.
        let height = self.effective_height(&ctx.theme);
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(160.0, height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect { origin: Point { x: lx, y: ly }, size: ls }));
            lx += ls.width + 12.0;
        }

        // Actions (right side) — paint right-to-left, tracking the left edge
        // so the title stops before them.
        let mut ax = r.origin.x + r.size.width - 12.0;
        for action in self.actions.iter().rev() {
            let as_ = action.layout(&ctx.layout_ctx(Constraints::loose(160.0, height)));
            ax -= as_.width + 6.0;
            let ay = r.origin.y + (r.size.height - as_.height) / 2.0;
            action.paint(&mut ctx.child(Rect { origin: Point { x: ax, y: ay }, size: as_ }));
        }

        // Title — the space BETWEEN leading and actions is always the clip
        // region (the title must never overlap either), but WHERE within the
        // full bar it centers depends on the theme (D105):
        //   Leading (default, unchanged from pre-D105 behavior) — centered
        //     within that between-leading-and-actions region, falling back
        //     to left-aligned when it doesn't fit.
        //   Center (Cupertino) — centered in the FULL bar width, still
        //     clipped to the between region so it can't overlap leading/
        //     actions — the iOS convention.
        let region_l = lx;
        let region_r = (ax - 8.0).max(region_l);
        let region_w = (region_r - region_l).max(0.0);
        if region_w > 4.0 {
            let title_w = ctx.font.measure_text(&self.title, self.title_size);
            let line_h = ctx.font.line_height(self.title_size);
            let title_y = r.origin.y + (r.size.height - line_h) / 2.0;
            let title_x = match style.title_align {
                TitleAlign::Leading => {
                    if title_w <= region_w {
                        region_l + (region_w - title_w) / 2.0
                    } else {
                        region_l
                    }
                }
                TitleAlign::Center => {
                    let full_center = r.origin.x + (r.size.width - title_w) / 2.0;
                    full_center.clamp(region_l, (region_r - title_w).max(region_l))
                }
            };
            let clip = Rect {
                origin: Point { x: region_l, y: r.origin.y },
                size: Size { width: region_w, height: r.size.height },
            };
            ctx.record(DrawCommand::PushClip { rect: clip });
            ctx.draw_text_at(&self.title, Point { x: title_x, y: title_y }, fg, self.title_size);
            ctx.record(DrawCommand::PopClip);
        }
    }
}
