use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// Placement algorithm for a [`Grid`] (D115/Phase 32 Step 1).
///
/// Internal — selected through the [`Grid::staggered`] / [`Grid::bento`]
/// builders; the default (`Uniform`) is the original Grid behavior,
/// unchanged (Phase 32 Migration Rule: all additive).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum GridMode {
    /// Equal-width cells, row height = tallest child of that row (the
    /// original, default behavior).
    #[default]
    Uniform,
    /// Masonry: each child keeps its OWN measured height at the column
    /// width and drops into the currently-shortest column.
    Staggered,
    /// Fixed lattice: items span whole columns/rows (see
    /// [`Grid::child_span`]); every lattice row is [`Grid::row_height`]
    /// tall.
    Bento,
}

/// Default lattice row height for [`Grid::bento`] mode, in logical px.
const DEFAULT_BENTO_ROW_HEIGHT: f32 = 96.0;

/// A fixed-column grid. Children flow left→right, top→bottom into `columns`
/// equal-width cells; each row's height is its tallest child. Lays out
/// something new (not a Column/Row) — see D095.
///
/// Two additional placement modes (D115/Phase 32 Step 1):
/// - [`Grid::staggered`] — masonry packing (Pinterest-style): children keep
///   their own measured heights and fill the shortest column first.
/// - [`Grid::bento`] — a fixed lattice where children added via
///   [`Grid::child_span`] cover multiple columns/rows (dashboard tiles).
pub struct Grid {
    columns: usize,
    spacing: f32,
    run_spacing: f32,
    children: Vec<BoxedWidget>,
    /// Per-child `(col_span, row_span)`, parallel to `children`. Only
    /// consulted in [`GridMode::Bento`]; `(1, 1)` everywhere else.
    spans: Vec<(u16, u16)>,
    mode: GridMode,
    /// Lattice row height for bento mode (logical px).
    row_height: f32,
}

impl Grid {
    /// A uniform grid with `columns` equal-width columns.
    pub fn new(columns: usize) -> Self {
        Self {
            columns: columns.max(1),
            spacing: 8.0,
            run_spacing: 8.0,
            children: Vec::new(),
            spans: Vec::new(),
            mode: GridMode::default(),
            row_height: DEFAULT_BENTO_ROW_HEIGHT,
        }
    }
    /// Horizontal gap between columns (logical px).
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    /// Vertical gap between rows (logical px).
    pub fn run_spacing(mut self, s: f32) -> Self { self.run_spacing = s; self }
    /// Append a child (span `1×1` in bento mode).
    pub fn child(mut self, w: impl Widget + 'static) -> Self {
        self.children.push(Box::new(w));
        self.spans.push((1, 1));
        self
    }
    /// Append several children (each span `1×1` in bento mode).
    pub fn children(mut self, ws: Vec<BoxedWidget>) -> Self {
        self.spans.extend(std::iter::repeat_n((1, 1), ws.len()));
        self.children.extend(ws);
        self
    }

    /// Switch to masonry placement: each child keeps its own measured
    /// height at the column width and is placed into the currently-shortest
    /// column (leftmost wins ties). Layout height = the tallest column.
    pub fn staggered(mut self) -> Self { self.mode = GridMode::Staggered; self }

    /// Switch to bento placement: children occupy whole cells of a fixed
    /// lattice (`columns` wide, rows of [`Grid::row_height`]), spanning
    /// multiple columns/rows per [`Grid::child_span`]. Items are placed
    /// first-fit: top-to-bottom, left-to-right, into the first free block
    /// that fits their span.
    pub fn bento(mut self) -> Self { self.mode = GridMode::Bento; self }

    /// Append a child spanning `col_span × row_span` lattice cells and
    /// switch to bento mode.
    ///
    /// A per-child span (rather than a parallel `.bento(Vec<(u16, u16)>)`
    /// span list) was chosen deliberately: the span lives at the same call
    /// site as the child it describes, so conditionally-added children can
    /// never silently desynchronize an index-aligned spans vector.
    pub fn child_span(mut self, w: impl Widget + 'static, col_span: u16, row_span: u16) -> Self {
        self.mode = GridMode::Bento;
        self.children.push(Box::new(w));
        self.spans.push((col_span.max(1), row_span.max(1)));
        self
    }

    /// Lattice row height for bento mode (logical px, default `96.0`).
    pub fn row_height(mut self, h: f32) -> Self { self.row_height = h.max(1.0); self }

    fn cell_width(&self, total: f32) -> f32 {
        let gaps = self.spacing * (self.columns.saturating_sub(1)) as f32;
        ((total - gaps) / self.columns as f32).max(0.0)
    }

    /// Measured cell sizes + total height for a given available width
    /// (uniform mode).
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

    /// Masonry placement: per-child rects (relative to the grid origin) +
    /// total content height. Each child is measured at the column width,
    /// keeps its own height, and goes into the currently-shortest column.
    fn arrange_staggered(&self, ctx: &LayoutCtx, width: f32) -> (Vec<Rect>, f32) {
        let cw = self.cell_width(width);
        let mut col_h = vec![0.0f32; self.columns];
        let mut rects = Vec::with_capacity(self.children.len());
        for c in &self.children {
            let s = c.layout(&ctx.with_constraints(Constraints::loose(cw, f32::INFINITY)));
            // Shortest column; leftmost wins ties (the masonry convention).
            let mut col = 0;
            for (i, h) in col_h.iter().enumerate().skip(1) {
                if *h < col_h[col] { col = i; }
            }
            let x = col as f32 * (cw + self.spacing);
            rects.push(Rect {
                origin: Point { x, y: col_h[col] },
                size: Size { width: cw, height: s.height },
            });
            col_h[col] += s.height + self.run_spacing;
        }
        let tallest = col_h.iter().fold(0.0_f32, |a, &h| a.max(h));
        (rects, (tallest - self.run_spacing).max(0.0))
    }

    /// Bento placement: per-child rects (relative) + total content height.
    /// First-fit on a `columns`-wide lattice of `row_height`-tall rows.
    fn arrange_bento(&self, width: f32) -> (Vec<Rect>, f32) {
        let cw = self.cell_width(width);
        // Occupancy lattice — grown row-by-row as placements demand.
        let mut occ: Vec<Vec<bool>> = Vec::new();
        let mut rects = Vec::with_capacity(self.children.len());
        let mut rows_used = 0usize;

        for i in 0..self.children.len() {
            let (cs, rs) = self.spans.get(i).copied().unwrap_or((1, 1));
            let cs = (cs as usize).clamp(1, self.columns);
            let rs = (rs as usize).max(1);

            let (row, col) = Self::first_fit(&occ, self.columns, cs, rs);
            // Grow the lattice and mark the block occupied.
            while occ.len() < row + rs { occ.push(vec![false; self.columns]); }
            for cells in occ.iter_mut().take(row + rs).skip(row) {
                for cell in cells.iter_mut().take(col + cs).skip(col) { *cell = true; }
            }
            rows_used = rows_used.max(row + rs);

            rects.push(Rect {
                origin: Point {
                    x: col as f32 * (cw + self.spacing),
                    y: row as f32 * (self.row_height + self.run_spacing),
                },
                size: Size {
                    width: cs as f32 * cw + (cs - 1) as f32 * self.spacing,
                    height: rs as f32 * self.row_height + (rs - 1) as f32 * self.run_spacing,
                },
            });
        }

        let total = if rows_used == 0 {
            0.0
        } else {
            rows_used as f32 * self.row_height + (rows_used - 1) as f32 * self.run_spacing
        };
        (rects, total)
    }

    /// First lattice position `(row, col)` where a `cs × rs` block fits.
    /// Always terminates: every row at/after `occ.len()` is empty.
    fn first_fit(occ: &[Vec<bool>], columns: usize, cs: usize, rs: usize) -> (usize, usize) {
        for row in 0..=occ.len() {
            for col in 0..=(columns - cs) {
                let fits = (row..row + rs).all(|r| {
                    occ.get(r).is_none_or(|cells| !cells[col..col + cs].iter().any(|&o| o))
                });
                if fits { return (row, col); }
            }
        }
        (occ.len(), 0) // unreachable — the all-empty `occ.len()` row always fits
    }

    /// Relative child rects + content height for the non-uniform modes.
    fn arrange(&self, ctx: &LayoutCtx, width: f32) -> (Vec<Rect>, f32) {
        match self.mode {
            GridMode::Staggered => self.arrange_staggered(ctx, width),
            // Uniform never routes here (kept on its original row-based
            // path in layout/paint); bento is the only other arm.
            _ => self.arrange_bento(width),
        }
    }
}

impl Widget for Grid {
    fn children(&self) -> Children<'_> { Children::Many(&self.children) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = avail_w(ctx.constraints);
        let h = match self.mode {
            GridMode::Uniform => self.measure(ctx, w).1,
            _ => self.arrange(ctx, w).1,
        };
        ctx.constraints.constrain(Size { width: w, height: h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        if self.mode != GridMode::Uniform {
            let (rects, _) = self.arrange(
                &ctx.layout_ctx(Constraints::loose(r.size.width, f32::INFINITY)),
                r.size.width,
            );
            for (child, rel) in self.children.iter().zip(rects) {
                let rect = Rect {
                    origin: Point { x: r.origin.x + rel.origin.x, y: r.origin.y + rel.origin.y },
                    size: rel.size,
                };
                child.paint(&mut ctx.child(rect));
            }
            return;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A leaf reporting a fixed size regardless of constraints.
    struct Fixed(f32, f32);
    impl Widget for Fixed {
        fn layout(&self, _ctx: &LayoutCtx) -> Size {
            Size { width: self.0, height: self.1 }
        }
        fn paint(&self, _ctx: &mut PaintCtx) {}
    }

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn staggered_packs_items_into_the_shortest_column() {
        // 2 columns, no gaps, 300px wide → 150px cells. Heights 40/100/20/30:
        // item0 → col0 (y 0), item1 → col1 (y 0), item2 → col0 (y 40,
        // shortest), item3 → col0 again (y 60; col0 = 60 < col1 = 100).
        let grid = Grid::new(2)
            .spacing(0.0)
            .run_spacing(0.0)
            .staggered()
            .child(Fixed(150.0, 40.0))
            .child(Fixed(150.0, 100.0))
            .child(Fixed(150.0, 20.0))
            .child(Fixed(150.0, 30.0));
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        let (rects, height) = grid.arrange_staggered(&ctx, 300.0);

        assert_eq!((rects[0].origin.x, rects[0].origin.y), (0.0, 0.0));
        assert_eq!((rects[1].origin.x, rects[1].origin.y), (150.0, 0.0));
        assert_eq!((rects[2].origin.x, rects[2].origin.y), (0.0, 40.0));
        assert_eq!((rects[3].origin.x, rects[3].origin.y), (0.0, 60.0));
        // Tallest column: col1 at 100 (col0 ends at 90).
        assert_eq!(height, 100.0);
        assert_eq!(grid.layout(&ctx).height, 100.0);
    }

    #[test]
    fn staggered_children_keep_their_own_heights() {
        let grid = Grid::new(2)
            .spacing(0.0)
            .run_spacing(0.0)
            .staggered()
            .child(Fixed(150.0, 40.0))
            .child(Fixed(150.0, 100.0));
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        let (rects, _) = grid.arrange_staggered(&ctx, 300.0);
        assert_eq!(rects[0].size.height, 40.0);
        assert_eq!(rects[1].size.height, 100.0);
    }

    #[test]
    fn bento_honors_column_and_row_spans() {
        // 2 columns, no gaps, 200px wide → 100px cells, 50px lattice rows.
        // item0 spans 2×1 (full first row), item1/item2 fill row 1,
        // item3 spans 1×2 (rows 2-3, col 0).
        let grid = Grid::new(2)
            .spacing(0.0)
            .run_spacing(0.0)
            .row_height(50.0)
            .child_span(Fixed(1.0, 1.0), 2, 1)
            .child_span(Fixed(1.0, 1.0), 1, 1)
            .child_span(Fixed(1.0, 1.0), 1, 1)
            .child_span(Fixed(1.0, 1.0), 1, 2);
        let (rects, height) = grid.arrange_bento(200.0);

        assert_eq!((rects[0].origin.x, rects[0].origin.y), (0.0, 0.0));
        assert_eq!((rects[0].size.width, rects[0].size.height), (200.0, 50.0));
        assert_eq!((rects[1].origin.x, rects[1].origin.y), (0.0, 50.0));
        assert_eq!((rects[2].origin.x, rects[2].origin.y), (100.0, 50.0));
        assert_eq!((rects[3].origin.x, rects[3].origin.y), (0.0, 100.0));
        assert_eq!((rects[3].size.width, rects[3].size.height), (100.0, 100.0));
        // 4 lattice rows × 50px.
        assert_eq!(height, 200.0);
    }

    #[test]
    fn bento_first_fit_backfills_gaps_beside_tall_items() {
        // 2 columns: item0 is 1×2 (col 0, rows 0-1); item1 (1×1) must land
        // beside it at (row 0, col 1), not below it.
        let grid = Grid::new(2)
            .spacing(0.0)
            .run_spacing(0.0)
            .row_height(50.0)
            .child_span(Fixed(1.0, 1.0), 1, 2)
            .child_span(Fixed(1.0, 1.0), 1, 1);
        let (rects, height) = grid.arrange_bento(200.0);
        assert_eq!((rects[1].origin.x, rects[1].origin.y), (100.0, 0.0));
        assert_eq!(height, 100.0);
    }

    #[test]
    fn uniform_default_behavior_is_unchanged() {
        // Regression guard for the Migration Rule: a plain Grid::new still
        // lays out row-by-row with row height = tallest child.
        let grid = Grid::new(2)
            .spacing(0.0)
            .run_spacing(0.0)
            .child(Fixed(150.0, 40.0))
            .child(Fixed(150.0, 100.0))
            .child(Fixed(150.0, 20.0));
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        // Row 0 = max(40, 100) = 100; row 1 = 20 → 120 total.
        assert_eq!(grid.layout(&ctx).height, 120.0);
    }
}
