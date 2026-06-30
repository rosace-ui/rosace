use std::sync::Mutex;
use tezzera_core::types::Size;
use tezzera_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment, layout_row};
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
        let fixed_w: f32 = self.children.iter()
            .filter(|c| c.flex_factor() == 0.0)
            .map(|c| c.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h))).width)
            .sum::<f32>() + gap_total;

        let flex_pool = (max_w - fixed_w).max(0.0);

        let sizes: Vec<Size> = self.children.iter().map(|c| {
            let ff = c.flex_factor();
            if ff > 0.0 && total_flex > 0.0 {
                let w = flex_pool * ff / total_flex;
                c.layout(&ctx.with_constraints(Constraints::tight(w, max_h)))
            } else {
                c.layout(&ctx.with_constraints(Constraints::loose(max_w, max_h)))
            }
        }).collect();

        *self.measure_cache.lock().unwrap() = Some((c, sizes.clone()));
        sizes
    }
}

impl Default for Row {
    fn default() -> Self { Self::new() }
}

impl Widget for Row {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let sizes = self.measure(ctx);
        let c = ctx.constraints;
        let inner_c = Constraints::loose(
            (avail_w(c) - self.padding.total_h()).max(0.0),
            (avail_h(c) - self.padding.total_v()).max(0.0),
        );
        let result = layout_row(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        self.padding.grow(result.size)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let inner_rect = self.padding.shrink(ctx.rect);
        let inner_c = Constraints::loose(inner_rect.size.width, inner_rect.size.height);
        let lctx = ctx.layout_ctx(inner_c);
        let sizes = self.measure(&lctx);
        let result = layout_row(inner_c, &sizes,
            self.main_axis_alignment, self.cross_axis_alignment, self.spacing);
        for (i, child) in self.children.iter().enumerate() {
            let pos = result.child_positions[i];
            let child_rect = rect_at(offset(inner_rect.origin, pos.x, pos.y), sizes[i]);
            child.paint(&mut ctx.child(child_rect));
        }
    }
}
