use std::sync::Arc;
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
    measure_cache: Mutex<Option<(u64, Constraints, Vec<Size>)>>,
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
        self.children.push(Arc::new(w)); self
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

    /// Per-child sizes for a given constraint, for tests that need to observe
    /// the measure pass directly — the cross-axis rules in particular, which a
    /// containing widget may bound before they are reached.
    pub fn layout_sizes_for_test(&self, ctx: &LayoutCtx) -> Vec<Size> {
        self.measure(ctx)
    }

    fn measure(&self, ctx: &LayoutCtx) -> Vec<Size> {
        let c = ctx.constraints;
        {
            let cache = self.measure_cache.lock().unwrap();
            if let Some((frame, cached_c, ref sizes)) = *cache {
                // Same frame AND same constraints. Dropping the frame check
                // makes this a cross-frame cache, which then returns a child's
                // stale size after that child re-laid-out.
                if frame == super::frame_id() && cached_c == c {
                    return sizes.clone();
                }
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
        // Measure each non-flex child ONCE and KEEP the whole `Size`.
        //
        // This pass used to take `.width` and discard the rest, so the
        // `sizes` pass below re-measured the same child with the identical
        // constraints to recover it — every non-flex child laid out twice per
        // frame, and the cost multiplied with nesting depth (a 4-deep flex
        // tree measured its leaves 16 times). Row and Column are the most
        // used containers in any app, so this was the hot path.
        let mut measured: Vec<Option<Size>> = vec![None; self.children.len()];
        let mut fixed_w: f32 = gap_total;
        for (i, child) in self.children.iter().enumerate() {
            if !flex_enabled || child.flex_factor() == 0.0 {
                let s = ctx.layout_child_at(i, Constraints::loose(max_w, max_h), &**child);
                fixed_w += s.width;
                measured[i] = Some(s);
            }
        }

        let flex_pool = (max_w - fixed_w).max(0.0);
        // `c` is shadowed by the closure's child parameter below.
        let cross_bound = c.max_height;

        let sizes: Vec<Size> = self.children.iter().enumerate().map(|(i, c)| {
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
                ctx.layout_child_at(i, Constraints {
                    min_width: w,
                    max_width: rosace_core::AxisBound::Bounded(w),
                    min_height: 0.0,
                    max_height: cross_bound,
                }, &**c)
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
                    None => ctx.layout_child_at(i, Constraints::loose(max_w, max_h), &**c),
                }
            }
        }).collect();


        // `Stretch` means children FILL the cross axis. `layout_row` already
        // sizes the container for it, but a child only becomes that tall if it
        // is MEASURED that way — without this pass `Stretch` was declared in
        // the enum and honoured nowhere, silently doing nothing.
        //
        // Only meaningful with a bounded cross axis. "Fill the available
        // height" has no answer inside a vertical `ScrollView`, which offers
        // infinity; Flutter raises on exactly this. Warn and leave the sizes
        // alone rather than inventing a bound, and rather than staying silent
        // — silence is what let a stretched `Row` holding a vertical `Divider`
        // reach the layout assertion with `Size { height: inf }` and take the
        // app down.
        let sizes = if self.cross_axis_alignment == CrossAxisAlignment::Stretch
            && !sizes.is_empty()
        {
            if max_h.is_finite() {
                self.children.iter().enumerate().map(|(i, ch)| {
                    // Main axis pinned to what it already resolved to; cross
                    // axis tight. Re-running the main-axis decision here would
                    // undo the flex distribution above.
                    let s = ctx.layout_child_at(i, Constraints {
                        min_width:  sizes[i].width,
                        max_width:  rosace_core::AxisBound::Bounded(sizes[i].width),
                        min_height: max_h,
                        max_height: rosace_core::AxisBound::Bounded(max_h),
                    }, &**ch);
                    // The cross extent is the PARENT's decision under Stretch,
                    // so take it rather than whatever the child returned. Tight
                    // constraints alone are not enough: the `Widget` trait does
                    // not force a child to honour its minimum, and a child that
                    // only clamps to the maximum reports its natural size and
                    // silently defeats the alignment.
                    Size { width: s.width, height: max_h }
                }).collect()
            } else {
                #[cfg(debug_assertions)]
                {
                    static WARNED_STRETCH: std::sync::Once = std::sync::Once::new();
                    WARNED_STRETCH.call_once(|| {
                        eprintln!(
                            "[ROSACE] Row: CrossAxisAlignment::Stretch inside an \
                             unbounded height (e.g. a vertical ScrollView) — there \
                             is no available height to fill, so children keep their \
                             own. Give the Row a bounded height, or size the child \
                             explicitly."
                        );
                    });
                }
                sizes
            }
        } else {
            sizes
        };
        *self.measure_cache.lock().unwrap() = Some((super::frame_id(), c, sizes.clone()));
        sizes
    }

    /// Paint-path sizes: reuse whatever layout() measured this frame — see
    /// Column::layout_sizes for why paint must never re-measure.
    fn layout_sizes(&self, ctx: &LayoutCtx) -> Vec<Size> {
        if let Some((frame, _, sizes)) = &*self.measure_cache.lock().unwrap() {
            if *frame == super::frame_id() {
                return sizes.clone();
            }
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
                let main = if c.constraints.min_width > 0.0 {
                    c.constraints.min_width
                } else {
                    self.1
                };
                Size { width: main, height: self.1 }
            }
            fn paint(&self, _ctx: &mut PaintCtx) {}
        }

        let hits = Arc::new(AtomicUsize::new(0));
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(rosace_layout::Constraints::loose(300.0, 300.0), &font, &theme);

        let w = Row::new()
            .child(CountingBox(hits.clone(), 50.0))                       // fixed
            .child(super::super::Expanded::new(CountingBox(hits.clone(), 10.0))); // flex

        let sizes = w.layout_sizes(&ctx);
        assert_eq!(sizes.len(), 2);

        // The FIXED child keeps its own size; the FLEX child takes the rest.
        assert_eq!(sizes[0].width, 50.0, "the fixed child is unchanged");
        assert_eq!(sizes[1].width, 300.0 - 50.0, "the flex child takes the remainder");

        // Two children, each measured once. Before this fix the fixed child
        // was measured twice.
        assert_eq!(hits.load(Ordering::SeqCst), 2,
            "each child measured exactly once");
    }

}
