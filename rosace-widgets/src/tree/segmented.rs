use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A one-of-N horizontal selector — segments in a rounded track, the selected
/// one highlighted. `on_change(index)` fires on tap.
pub struct SegmentedControl {
    segments: Vec<String>,
    selected: usize,
    disabled: bool,
    height: f32,
    track_color: Option<Color>,
    selected_color: Option<Color>,
    color: Option<Color>,
    selected_text_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl SegmentedControl {
    pub fn new(segments: Vec<impl Into<String>>, selected: usize) -> Self {
        Self {
            segments: segments.into_iter().map(Into::into).collect(), selected, disabled: false, height: 34.0,
            track_color: None, selected_color: None, color: None, selected_text_color: None,
            on_change: None,
        }
    }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    /// Track (unselected background) color — theme's `surface_variant` if unset.
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = Some(c); self }
    /// Selected pill's fill color — theme's `primary` if unset.
    pub fn selected_color(mut self, c: Color) -> Self { self.selected_color = Some(c); self }
    /// Unselected label color — theme's `on_surface` if unset.
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    /// Selected label color — theme's `on_primary` if unset.
    pub fn selected_text_color(mut self, c: Color) -> Self { self.selected_text_color = Some(c); self }
    pub fn on_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f)); self
    }
}

impl Widget for SegmentedControl {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w: f32 = self.segments.iter()
            .map(|s| ctx.font.measure_text(s, 13.0) + 28.0)
            .sum();
        ctx.constraints.constrain(Size { width: w.max(120.0), height: self.height })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let n = self.segments.len().max(1);
        let seg_w = r.size.width / n as f32;
        let radius = self.height / 2.0;
        let (track, fg, sel_bg, sel_fg) = {
            let t = &ctx.theme.colors;
            (self.track_color.unwrap_or_else(|| ctx.tc(t.surface_variant)),
             self.color.unwrap_or_else(|| ctx.tc(t.on_surface)),
             self.selected_color.unwrap_or_else(|| ctx.tc(t.primary)),
             self.selected_text_color.unwrap_or_else(|| ctx.tc(t.on_primary)))
        };
        let dim = if self.disabled { 0.45 } else { 1.0 };
        let with_alpha = |c: Color, a: f32| Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8);
        let focused = !self.disabled && ctx.focus_node().is_focused();

        // Track
        ctx.fill_rrect(r, radius, with_alpha(track, dim));

        // The highlight pill slides between segments (eased index position).
        let pos = ctx.animate_to(self.selected as f32, 0.0);
        let px = r.origin.x + pos * seg_w;
        let pill = Rect {
            origin: Point { x: px + 3.0, y: r.origin.y + 3.0 },
            size: Size { width: seg_w - 6.0, height: r.size.height - 6.0 },
        };
        ctx.fill_rrect(pill, radius - 3.0, with_alpha(sel_bg, dim));

        for (i, label) in self.segments.iter().enumerate() {
            let x = r.origin.x + i as f32 * seg_w;
            let seg_rect = Rect { origin: Point { x, y: r.origin.y }, size: Size { width: seg_w, height: r.size.height } };
            let mut child = ctx.child(seg_rect);

            // Per-segment hover/press wash on the UNSELECTED segments (the
            // selected one already reads as active via the pill).
            let hov = !self.disabled && child.hovered();
            let prs = !self.disabled && child.pressed();
            let nearness = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            if (hov || prs) && nearness < 0.5 {
                let wash = if prs { 0.12 } else { 0.07 };
                child.fill_rrect(Rect {
                    origin: Point { x: x + 3.0, y: r.origin.y + 3.0 },
                    size: Size { width: seg_w - 6.0, height: r.size.height - 6.0 },
                }, radius - 3.0, with_alpha(Color::rgb(255, 255, 255), wash));
            }

            let tw = child.font.measure_text(label, 13.0);
            let lh = child.font.line_height(13.0);
            let tx = x + (seg_w - tw) / 2.0;
            let ty = r.origin.y + (r.size.height - lh) / 2.0;
            // Text color blends toward selected as the pill nears this segment.
            let col = super::lerp_color(fg, sel_fg, nearness);
            child.draw_text_at(label, Point { x: tx, y: ty }, with_alpha(col, dim), 13.0);
            child.semantics(
                super::Semantics::new(rosace_core::Role::Tab)
                    .label(label)
                    .value(if i == self.selected { "selected" } else { "not selected" }),
            );
            match (&self.on_change, self.disabled) {
                (Some(cb), false) => { let cb = cb.clone(); let idx = i; child.register_hit(Arc::new(move || cb(idx))); }
                _ => child.register_hit(Arc::new(|| {})),
            }
        }

        // Focus ring around the whole control.
        if focused {
            let with_alpha = |c: Color, a: f32| Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8);
            ctx.stroke_rrect(r, radius, with_alpha(sel_bg, 0.9), 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn customization_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = SegmentedControl::new(vec!["Day", "Week"], 0);
        let customized = SegmentedControl::new(vec!["Day", "Week"], 0)
            .track_color(Color::rgb(10, 10, 10))
            .selected_color(Color::rgb(255, 0, 0))
            .color(Color::rgb(255, 255, 255))
            .selected_text_color(Color::rgb(0, 0, 0));
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }
}
