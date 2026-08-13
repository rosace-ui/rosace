use rosace_core::types::{Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};

/// A horizontal progress bar (0.0 – 1.0).
pub struct ProgressBar {
    pub value: f32,
    /// `None` = the active theme's `colors.surface_variant`. This used to be
    /// a hardcoded dark-theme blue-grey — the file read `ctx.theme` nowhere
    /// at all, so the bar looked wrong in a light theme.
    pub track_color: Option<Color>,
    /// `None` = the active theme's `colors.primary`.
    pub fill_color: Option<Color>,
    pub height: f32,
    /// `None` = fully rounded ends (`height / 2`), which tracks `.height()`
    /// instead of stranding a fixed 3.0 that only suited the default bar.
    pub radius: Option<f32>,
    pub width: Option<f32>,
    /// Accessible name. A `ProgressBar` announces a percentage; without this
    /// a screen reader says "50%" with no idea what is progressing.
    pub label: Option<String>,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            track_color: None,
            fill_color: None,
            height: 6.0,
            radius: None,
            width: None,
            label: None,
        }
    }
    pub fn color(mut self, c: Color) -> Self { self.fill_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = Some(c); self }
    /// Corner radius of both the track and the fill. Defaults to fully
    /// rounded ends.
    pub fn radius(mut self, r: f32) -> Self { self.radius = Some(r); self }
    /// What is progressing ("Uploading photos"), for assistive tech.
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
}

impl Widget for ProgressBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size {
            width:  self.width.unwrap_or(avail_w(constraints)),
            height: self.height,
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let mut sem = super::SemanticsProps::new(rosace_core::Role::ProgressBar)
            .value(format!("{:.0}%", self.value * 100.0));
        if let Some(l) = &self.label {
            sem = sem.label(l);
        }
        ctx.semantics(sem);
        use super::container::draw_rounded_rect_pub;
        let r = ctx.rect;
        let track = self.track_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.surface_variant));
        let fill_color = self.fill_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let radius = self.radius.unwrap_or(r.size.height / 2.0);
        // Track
        draw_rounded_rect_pub(ctx, r, track, radius);
        // Fill
        if self.value > 0.001 {
            // Never round the fill harder than it is wide, or a barely
            // started bar paints a lozenge wider than its own geometry.
            let w = r.size.width * self.value;
            let fill = Rect {
                origin: r.origin,
                size: Size { width: w, height: r.size.height },
            };
            draw_rounded_rect_pub(ctx, fill, fill_color, radius.min(w / 2.0));
        }
    }
}
