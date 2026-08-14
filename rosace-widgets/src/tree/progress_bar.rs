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

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::Point;
    use rosace_render::{DrawCommand, FontCache, PictureRecorder};
    use crate::tree::{PaintCtx, RenderTree};
    use std::{cell::RefCell, rc::Rc};

    /// Every rounded rect drawn, in paint order: track first, then fill.
    fn rrects(bar: ProgressBar, w: f32) -> Vec<(Rect, f32)> {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: w, height: 6.0 } },
                &font,
                rosace_theme::built_in::dark_theme(),
                Rc::new(RefCell::new(RenderTree::new())),
            );
            bar.paint(&mut ctx);
        }
        rec.finish().commands.iter().filter_map(|c| match c {
            DrawCommand::FillRRect { rect, radius, .. } => Some((*rect, *radius)),
            _ => None,
        }).collect()
    }

    #[test]
    fn the_fill_width_is_the_value_fraction_of_the_track() {
        let parts = rrects(ProgressBar::new(0.25), 200.0);
        assert_eq!(parts.len(), 2, "track + fill");
        assert_eq!(parts[0].0.size.width, 200.0, "track spans the full width");
        assert_eq!(parts[1].0.size.width, 50.0, "fill is 25%");
    }

    /// A zero (or negative) value must draw NO fill, not a zero-width sliver
    /// that still renders as a rounded dot at the left edge.
    #[test]
    fn a_zero_value_draws_the_track_only() {
        assert_eq!(rrects(ProgressBar::new(0.0), 200.0).len(), 1);
        assert_eq!(rrects(ProgressBar::new(-5.0), 200.0).len(), 1, "clamped, not negative");
    }

    #[test]
    fn a_value_above_one_is_clamped_to_the_track() {
        let parts = rrects(ProgressBar::new(3.0), 200.0);
        assert_eq!(parts[1].0.size.width, 200.0, "never overruns the track");
    }

    /// The fill must not be rounded harder than it is wide.
    ///
    /// The default radius is half the HEIGHT, so at a low value the fill was
    /// narrower than its own corner radius and painted a lozenge wider than
    /// the geometry implied — progress appearing to start at ~3% when the
    /// value was 0.5%.
    #[test]
    fn a_barely_started_fill_is_not_rounded_wider_than_itself() {
        let parts = rrects(ProgressBar::new(0.005), 200.0);
        let (fill, radius) = parts[1];
        assert!(radius <= fill.size.width / 2.0 + 0.001,
            "radius {radius} exceeds half the {}px fill", fill.size.width);
    }

    /// A ProgressBar announces a percentage; without a name a screen reader
    /// says "50%" and nothing else.
    #[test]
    fn it_announces_its_value_and_an_explicit_label() {
        let font = FontCache::embedded();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 200.0, height: 6.0 } },
                &font,
                rosace_theme::built_in::dark_theme(),
                tree.clone(),
            );
            ProgressBar::new(0.5).label("Uploading photos").paint(&mut ctx);
        }
        let json = format!("{:?}", tree.borrow().collect_semantics());
        assert!(json.contains("Uploading photos"), "the label must be announced: {json}");
        assert!(json.contains("50%"), "the value must be announced");
    }
}
