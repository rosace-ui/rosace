use std::sync::Mutex;
use rosace_core::types::Size;
use rosace_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment, layout_row};
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h, offset, rect_at};
use super::padding::EdgeInsets;

/// Horizontal flex container. Children are arranged left-to-right.
///
/// [`Expanded`] children automatically receive leftover horizontal space.
///
/// [`Expanded`]: super::spacer::Expanded
pub struct Row {
    children: Vec<BoxedWidget>,
    spacing: f32,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    padding: EdgeInsets,
    measure_cache: Mutex<Option<(Constraints, Vec<Size>)>>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            spacing: 0.0,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Center,
            padding: EdgeInsets::default(),
            measure_cache: Mutex::new(None),
        }
    }

    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn main_axis_alignment(mut self, a: MainAxisAlignment) -> Self { self.main_axis_alignment = a; self }
    pub fn cross_axis_alignment(mut self, a: CrossAxisAlignment) -> Self { self.cross_axis_alignment = a; self }

    pub fn child(mut self, w: impl Widget + 'static) -> Self {
        self.children.push(Box::new(w)); self
    }
    pub fn children(mut self, ws: Vec<BoxedWidget>) -> Self {
        self.children.extend(ws); self
    }

    /// Wrap this flex container in a ScrollView scrolling horizontally
    /// (D101: position is implicit per-node state — zero wiring).
    /// Expanded children are ignored on the unbounded scroll axis.
    pub fn scrollable(self) -> super::ScrollView {
        super::ScrollView::horizontal(self)
    }

    fn measure(&self, ctx: &LayoutCtx) -> Vec<Size> {
        let c = ctx.constraints;
        {
            let cache = self.measure_cache.lock().unwrap();
            if let Some((cached_c, ref sizes)) = *cache {
                if cached_c == c { return sizes.clone(); }
            }
        }

        let max_w = (avail_w(c) - self.padding.total_h()).max(0.0);
        let max_h = (avail_h(c) - self.padding.total_v()).max(0.0);
        let n = self.children.len();
        let gap_total = if n > 1 { self.spacing * (n - 1) as f32 } else { 0.0 };

        let total_flex: f32 = self.children.iter().map(|c| c.flex_factor()).sum();
        // Unbounded-axis doctrine (API_DESIGN §6): flex needs a finite main
        // axis. Inside a horizontal ScrollView, Expanded children size to
        // content instead of erroring.
        let flex_enabled = total_flex > 0.0 && max_w.is_finite();
        #[cfg(debug_assertions)]
        if total_flex > 0.0 && !flex_enabled {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                eprintln!(
                    "[ROSACE] Row: Expanded child inside an unbounded width \
                     (e.g. a horizontal ScrollView) — flex is ignored, the child \
                     sizes to its content. Give the Row a bounded width to flex."
                );
            });
        }
        let fixed_w: f32 = self.children.iter()
            .filter(|c| !flex_enabled || c.flex_factor() == 0.0)
            .map(|c| c.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h))).width)
            .sum::<f32>() + gap_total;

        let flex_pool = (max_w - fixed_w).max(0.0);
        // `c` is shadowed by the closure's child parameter below.
        let cross_bound = c.max_height;

        let sizes: Vec<Size> = self.children.iter().map(|c| {
            let ff = c.flex_factor();
            if ff > 0.0 && flex_enabled {
                let w = flex_pool * ff / total_flex;
                // Tight on the MAIN axis (that is the flex share) but the
                // cross axis keeps the parent's real bound. `max_h` is
                // `f32::INFINITY` whenever the height is unbounded — inside
                // any vertical ScrollView — and `Constraints::tight(w, inf)`
                // FORCES an infinite height on the child. A child that
                // centres its own content then computes `(inf - h) / 2`,
                // which is `inf`, and the subsequent arithmetic yields NaN:
                // the text was drawn at y = NaN and the glyph walk panicked
                // on an out-of-range cast. Found by the showcase's
                // paint-every-page test.
                c.layout(&ctx.with_constraints(Constraints {
                    min_width: w,
                    max_width: rosace_core::AxisBound::Bounded(w),
                    min_height: 0.0,
                    max_height: cross_bound,
                }))
            } else {
                c.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h)))
            }
        }).collect();

        *self.measure_cache.lock().unwrap() = Some((c, sizes.clone()));
        sizes
    }

    /// Paint-path sizes: reuse whatever layout() measured this frame — see
    /// Column::layout_sizes for why paint must never re-measure.
    fn layout_sizes(&self, ctx: &LayoutCtx) -> Vec<Size> {
        if let Some((_, sizes)) = &*self.measure_cache.lock().unwrap() {
            return sizes.clone();
        }
        self.measure(ctx)
    }
}

impl Default for Row {
    fn default() -> Self { Self::new() }
}

impl Widget for Row {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let sizes = self.measure(ctx);
        let c = ctx.constraints;
        // Preserve incoming minimums (unbounded-axis doctrine) so alignment
        // can distribute against a viewport-sized minimum.
        let (pad_h, pad_v) = (self.padding.total_h(), self.padding.total_v());
        let inner_c = Constraints {
            min_width:  (c.min_width - pad_h).max(0.0),
            max_width:  super::shrink_axis(c.max_width, pad_h),
            min_height: (c.min_height - pad_v).max(0.0),
            max_height: super::shrink_axis(c.max_height, pad_v),
        };
        let result = layout_row(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        self.padding.grow(result.size)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let inner_rect = self.padding.shrink(ctx.rect);
        // Tight to the allotted rect — measure and paint agree on extra space.
        let inner_c = Constraints::tight(inner_rect.size.width, inner_rect.size.height);
        let lctx = ctx.layout_ctx(inner_c);
        let sizes = self.layout_sizes(&lctx);
        let result = layout_row(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        for (i, child) in self.children.iter().enumerate() {
            let pos = result.child_positions[i];
            let child_rect = rect_at(offset(inner_rect.origin, pos.x, pos.y), sizes[i]);
            child.paint(&mut ctx.child(child_rect));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `Expanded` child inside an UNBOUNDED cross axis must not be handed
    /// an infinite size.
    ///
    /// `avail_h` reports `f32::INFINITY` for an unbounded axis — which is
    /// every Row inside a vertical `ScrollView`. Baking that into the flex
    /// branch's tight constraint forced an infinite height on the child; a
    /// child that centres its own content then computed `(inf - h) / 2`, and
    /// the arithmetic that followed produced NaN. The result was text drawn
    /// at NaN and a panic in the glyph walk's `as i32` cast — a crash, not a
    /// layout glitch.
    #[test]
    fn an_expanded_child_is_never_given_an_infinite_cross_axis() {
        use rosace_render::FontCache;
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();

        let w = Row::new().child(super::super::Expanded::new(
            super::super::Container::new().height(64.0),
        ));
        let c = Constraints {
            min_width: 0.0,
            max_width: rosace_core::AxisBound::Bounded(420.0),
            min_height: 0.0,
            max_height: rosace_core::AxisBound::Unbounded,
        };
        let sizes = w.layout_sizes(&LayoutCtx::new(c, &font, &theme));
        for s in &sizes {
            assert!(s.width.is_finite(), "child width must be finite, got {}", s.width);
            assert!(s.height.is_finite(), "child height must be finite, got {}", s.height);
        }
    }
}
