use std::sync::Arc;
use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A one-of-N horizontal selector — segments in a rounded track, the selected
/// one highlighted. `on_change(index)` fires on tap.
pub struct SegmentedControl {
    segments: Vec<String>,
    selected: usize,
    height: f32,
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl SegmentedControl {
    pub fn new(segments: Vec<impl Into<String>>, selected: usize) -> Self {
        Self { segments: segments.into_iter().map(Into::into).collect(), selected, height: 34.0, on_change: None }
    }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
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
        // Track
        ctx.fill_rrect(r, radius, ctx.tc(ctx.theme.colors.surface_variant));
        let fg = ctx.tc(ctx.theme.colors.on_surface);
        let sel_bg = ctx.tc(ctx.theme.colors.primary);
        let sel_fg = ctx.tc(ctx.theme.colors.on_primary);
        for (i, label) in self.segments.iter().enumerate() {
            let x = r.origin.x + i as f32 * seg_w;
            let seg_rect = Rect { origin: Point { x, y: r.origin.y }, size: Size { width: seg_w, height: r.size.height } };
            if i == self.selected {
                let pill = Rect {
                    origin: Point { x: x + 3.0, y: r.origin.y + 3.0 },
                    size: Size { width: seg_w - 6.0, height: r.size.height - 6.0 },
                };
                ctx.fill_rrect(pill, radius - 3.0, sel_bg);
            }
            let tw = ctx.font.measure_text(label, 13.0);
            let lh = ctx.font.line_height(13.0);
            let tx = x + (seg_w - tw) / 2.0;
            let ty = r.origin.y + (r.size.height - lh) / 2.0;
            ctx.draw_text_at(label, Point { x: tx, y: ty }, if i == self.selected { sel_fg } else { fg }, 13.0);
            if let Some(cb) = &self.on_change {
                let cb = cb.clone();
                let idx = i;
                ctx.child(seg_rect).register_hit(Arc::new(move || cb(idx)));
            }
        }
    }
}
