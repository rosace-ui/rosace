use std::sync::Mutex;
use rosace_core::types::Size;
use rosace_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment, layout_column};
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h, offset, rect_at};
use super::padding::EdgeInsets;

/// Vertical flex container. Children are arranged top-to-bottom.
///
/// [`Expanded`] children automatically receive the leftover vertical space.
///
/// [`Expanded`]: super::spacer::Expanded
pub struct Column {
    children: Vec<BoxedWidget>,
    spacing: f32,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    padding: EdgeInsets,
    measure_cache: Mutex<Option<(Constraints, Vec<Size>)>>,
}

impl Column {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            spacing: 0.0,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Start,
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

    /// Wrap this flex container in a ScrollView scrolling vertically
    /// (D101: position is implicit per-node state — zero wiring).
    /// Expanded children are ignored on the unbounded scroll axis.
    pub fn scrollable(self) -> super::ScrollView {
        super::ScrollView::new(self)
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
        // axis to divide. Inside a vertical ScrollView (max_height unbounded)
        // Expanded children are DEFINED to size to content — never a panic.
        let flex_enabled = total_flex > 0.0 && max_h.is_finite();
        #[cfg(debug_assertions)]
        if total_flex > 0.0 && !flex_enabled {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                eprintln!(
                    "[ROSACE] Column: Expanded child inside an unbounded height \
                     (e.g. a vertical ScrollView) — flex is ignored, the child \
                     sizes to its content. Give the Column a bounded height to flex."
                );
            });
        }
        // Measure each non-flex child ONCE and KEEP the whole `Size`.
        //
        // This pass used to take `.height` and discard the rest, so the
        // `sizes` pass below re-measured the same child with the identical
        // constraints to recover it — every non-flex child laid out twice per
        // frame, and the cost multiplied with nesting depth (a 4-deep flex
        // tree measured its leaves 16 times). Row and Column are the most
        // used containers in any app, so this was the hot path.
        let mut measured: Vec<Option<Size>> = vec![None; self.children.len()];
        let mut fixed_h: f32 = gap_total;
        for (i, child) in self.children.iter().enumerate() {
            if !flex_enabled || child.flex_factor() == 0.0 {
                let s = child.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h)));
                fixed_h += s.height;
                measured[i] = Some(s);
            }
        }

        let flex_pool = (max_h - fixed_h).max(0.0);
        // `c` is shadowed by the closure's child parameter below.
        let cross_bound = c.max_width;

        let sizes: Vec<Size> = self.children.iter().enumerate().map(|(i, c)| {
            let ff = c.flex_factor();
            if ff > 0.0 && flex_enabled {
                let h = flex_pool * ff / total_flex;
                // Mirror of the fix in `row.rs` — see its comment. `max_w` is
                // infinite inside a horizontal ScrollView, and baking that
                // into a tight constraint hands the child an infinite width.
                c.layout(&ctx.with_constraints(Constraints {
                    min_width: 0.0,
                    max_width: cross_bound,
                    min_height: h,
                    max_height: rosace_core::AxisBound::Bounded(h),
                }))
            } else {
                // Reuse the measurement from the pass above: same child, same
                // constraints, so the result is identical by construction.
                // Falls back to measuring rather than panicking if that
                // invariant is ever broken — a wrong size is a layout bug, a
                // panic in layout takes the app down.
                debug_assert!(measured[i].is_some(),
                    "every non-flex child should have been measured above");
                match measured[i] {
                    Some(s) => s,
                    None => c.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h))),
                }
            }
        }).collect();

        *self.measure_cache.lock().unwrap() = Some((c, sizes.clone()));
        sizes
    }

    /// Paint-path sizes: reuse whatever layout() measured this frame.
    ///
    /// Paint must NEVER re-measure under different constraints — the rect is
    /// always bounded, which would re-enable flex that layout disabled on an
    /// unbounded axis (children would change size between measure and paint).
    fn layout_sizes(&self, ctx: &LayoutCtx) -> Vec<Size> {
        if let Some((_, sizes)) = &*self.measure_cache.lock().unwrap() {
            return sizes.clone();
        }
        self.measure(ctx)
    }
}

impl Default for Column {
    fn default() -> Self { Self::new() }
}

impl Widget for Column {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let sizes = self.measure(ctx);
        let c = ctx.constraints;
        // Preserve incoming minimums (unbounded-axis doctrine): a ScrollView
        // hands its content min = viewport so MainAxisAlignment can center
        // short content against the full viewport.
        let (pad_h, pad_v) = (self.padding.total_h(), self.padding.total_v());
        let inner_c = Constraints {
            min_width:  (c.min_width - pad_h).max(0.0),
            max_width:  super::shrink_axis(c.max_width, pad_h),
            min_height: (c.min_height - pad_v).max(0.0),
            max_height: super::shrink_axis(c.max_height, pad_v),
        };
        let result = layout_column(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        self.padding.grow(result.size)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let inner_rect = self.padding.shrink(ctx.rect);
        // Tight to the allotted rect so alignment distributes the same extra
        // space that layout() reported — measure and paint agree.
        let inner_c = Constraints::tight(inner_rect.size.width, inner_rect.size.height);
        let lctx = ctx.layout_ctx(inner_c);
        let sizes = self.layout_sizes(&lctx);
        let result = layout_column(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        for (i, child) in self.children.iter().enumerate() {
            let pos = result.child_positions[i];
            let child_rect = rect_at(offset(inner_rect.origin, pos.x, pos.y), sizes[i]);
            ctx.paint_child(child_rect, &*child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `Expanded` child inside an UNBOUNDED cross axis must not be handed
    /// an infinite size.
    ///
    /// `avail_w` reports `f32::INFINITY` for an unbounded axis — which is
    /// every Column inside a horizontal `ScrollView`. Baking that into the flex
    /// branch's tight constraint forced an infinite width on the child; a
    /// child that centres its own content then computed `(inf - h) / 2`, and
    /// the arithmetic that followed produced NaN. The result was text drawn
    /// at NaN and a panic in the glyph walk's `as i32` cast — a crash, not a
    /// layout glitch.
    #[test]
    fn an_expanded_child_is_never_given_an_infinite_cross_axis() {
        use rosace_render::FontCache;
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();

        let w = Column::new().child(super::super::Expanded::new(
            super::super::Container::new().width(64.0),
        ));
        let c = Constraints {
            min_width: 0.0,
            max_width: rosace_core::AxisBound::Unbounded,
            min_height: 0.0,
            max_height: rosace_core::AxisBound::Bounded(800.0),
        };
        let sizes = w.layout_sizes(&LayoutCtx::new(c, &font, &theme));
        for s in &sizes {
            assert!(s.width.is_finite(), "child width must be finite, got {}", s.width);
            assert!(s.height.is_finite(), "child height must be finite, got {}", s.height);
        }
    }

    /// A non-flex child is measured ONCE, and mixing flex with non-flex
    /// still sizes both correctly.
    ///
    /// The two passes existed because the flex pool needs the fixed children
    /// measured first. Reusing that measurement is only safe because the
    /// second pass used identical constraints for those children — this pins
    /// both halves: the count, and that the flex split is unaffected.
    #[test]
    fn a_non_flex_child_is_measured_once_and_flex_still_splits_correctly() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct CountingBox(Arc<AtomicUsize>, f32);
        impl Widget for CountingBox {
            fn layout(&self, c: &LayoutCtx) -> Size {
                self.0.fetch_add(1, Ordering::SeqCst);
                // Honour a tight main axis, as any real widget does — that is
                // what makes the flex share observable at all.
                let main = if c.constraints.min_height > 0.0 {
                    c.constraints.min_height
                } else {
                    self.1
                };
                Size { height: main, width: self.1 }
            }
            fn paint(&self, _ctx: &mut PaintCtx) {}
        }

        let hits = Arc::new(AtomicUsize::new(0));
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(rosace_layout::Constraints::loose(300.0, 300.0), &font, &theme);

        let w = Column::new()
            .child(CountingBox(hits.clone(), 50.0))                       // fixed
            .child(super::super::Expanded::new(CountingBox(hits.clone(), 10.0))); // flex

        let sizes = w.layout_sizes(&ctx);
        assert_eq!(sizes.len(), 2);

        // The FIXED child keeps its own size; the FLEX child takes the rest.
        assert_eq!(sizes[0].height, 50.0, "the fixed child is unchanged");
        assert_eq!(sizes[1].height, 300.0 - 50.0, "the flex child takes the remainder");

        // Two children, each measured once. Before this fix the fixed child
        // was measured twice.
        assert_eq!(hits.load(Ordering::SeqCst), 2,
            "each child measured exactly once");
    }

}
