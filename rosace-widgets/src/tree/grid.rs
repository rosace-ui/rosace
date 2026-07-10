use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A fixed-column grid. Children flow left→right, top→bottom into `columns`
/// equal-width cells; each row's height is its tallest child. Lays out
/// something new (not a Column/Row) — see D095.
pub struct Grid {
    columns: usize,
    spacing: f32,
    run_spacing: f32,
    children: Vec<BoxedWidget>,
}

impl Grid {
    pub fn new(columns: usize) -> Self {
        Self { columns: columns.max(1), spacing: 8.0, run_spacing: 8.0, children: Vec::new() }
    }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn run_spacing(mut self, s: f32) -> Self { self.run_spacing = s; self }
    pub fn child(mut self, w: impl Widget + 'static) -> Self { self.children.push(Box::new(w)); self }
    pub fn children(mut self, ws: Vec<BoxedWidget>) -> Self { self.children.extend(ws); self }

    fn cell_width(&self, total: f32) -> f32 {
        let gaps = self.spacing * (self.columns.saturating_sub(1)) as f32;
        ((total - gaps) / self.columns as f32).max(0.0)
    }

    /// Measured cell sizes + total height for a given available width.
    fn measure(&self, ctx: &LayoutCtx, width: f32) -> (Vec<Size>, f32) {
        let cw = self.cell_width(width);
        let sizes: Vec<Size> = self.children.iter()
            .map(|c| c.layout(&ctx.with_constraints(Constraints::loose(cw, f32::INFINITY))))
            .collect();
        let mut y = 0.0;
        let mut i = 0;
        while i < sizes.len() {
            let row_h = sizes[i..(i + self.columns).min(sizes.len())]
                .iter().map(|s| s.height).fold(0.0_f32, f32::max);
            y += row_h;
            if i + self.columns < sizes.len() { y += self.run_spacing; }
            i += self.columns;
        }
        (sizes, y)
    }
}

impl Widget for Grid {
    fn children(&self) -> Children<'_> { Children::Many(&self.children) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = avail_w(ctx.constraints);
        let (_, h) = self.measure(ctx, w);
        ctx.constraints.constrain(Size { width: w, height: h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let cw = self.cell_width(r.size.width);
        let (sizes, _) = self.measure(&ctx.layout_ctx(Constraints::loose(r.size.width, r.size.height)), r.size.width);
        let mut y = r.origin.y;
        let mut i = 0;
        while i < self.children.len() {
            let end = (i + self.columns).min(self.children.len());
            let row_h = sizes[i..end].iter().map(|s| s.height).fold(0.0_f32, f32::max);
            for (col, idx) in (i..end).enumerate() {
                let x = r.origin.x + col as f32 * (cw + self.spacing);
                let rect = Rect { origin: Point { x, y }, size: Size { width: cw, height: row_h } };
                self.children[idx].paint(&mut ctx.child(rect));
            }
            y += row_h + self.run_spacing;
            i += self.columns;
        }
    }
}
