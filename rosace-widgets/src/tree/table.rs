//! `Table` (D115/Phase 32 Step 1) — a LAYOUT table: per-column sizing
//! (auto/fixed/flex) with row alignment, the widget-tree analogue of an
//! HTML `<table>` used purely for arrangement.
//!
//! Deliberately distinct from the future `DataTable` (the data-grid that
//! renders headers/sorting on top of a layout primitive like this one —
//! see PHASE_32.md's Out of Scope note). `Table` knows nothing about
//! data: rows are plain widget lists.
//!
//! Cells align top-left within their resolved column width; row height is
//! the tallest cell of that row.

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;

use super::{avail_w, BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// How a [`Table`] column resolves its width.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ColumnSizing {
    /// Width = the widest intrinsic (loose-measured) cell of the column.
    Auto,
    /// Width = a fixed number of logical px.
    Fixed(f32),
    /// Width = this factor's share of the space left after fixed/auto
    /// columns and gaps.
    Flex(f32),
}

/// Width policy for one [`Table`] column — construct via
/// [`TableColumn::auto`] / [`TableColumn::fixed`] / [`TableColumn::flex`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableColumn {
    sizing: ColumnSizing,
}

impl TableColumn {
    /// Column sized to its widest cell (each cell measured with loose
    /// constraints — its intrinsic width).
    pub fn auto() -> Self { Self { sizing: ColumnSizing::Auto } }
    /// Column with a fixed pixel width.
    pub fn fixed(px: f32) -> Self { Self { sizing: ColumnSizing::Fixed(px.max(0.0)) } }
    /// Column taking `factor`'s share of the leftover width (after fixed +
    /// auto columns and spacing). With an unbounded available width, flex
    /// columns fall back to their intrinsic (auto) width — there is no
    /// finite leftover to share.
    pub fn flex(factor: f32) -> Self { Self { sizing: ColumnSizing::Flex(factor.max(0.0)) } }
}

/// A layout table: declare columns with [`Table::column`], then add rows of
/// widgets with [`Table::row`]. Rows shorter than the column list leave the
/// remaining cells empty; extra cells beyond the column list are ignored.
pub struct Table {
    columns: Vec<TableColumn>,
    /// All cells, flattened row-major; `row_lens` records each row's length.
    cells: Vec<BoxedWidget>,
    row_lens: Vec<usize>,
    h_spacing: f32,
    v_spacing: f32,
    /// Uniform padding inside every cell (logical px).
    cell_padding: f32,
    /// Zebra striping: fill for every ODD row (1, 3, 5, …).
    row_background: Option<Color>,
    /// Hairline between rows; `0.0` = none.
    divider_width: f32,
    divider_color: Option<Color>,
}

impl Table {
    /// An empty table — add columns and rows with the builders below.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            cells: Vec::new(),
            row_lens: Vec::new(),
            h_spacing: 8.0,
            v_spacing: 8.0,
            cell_padding: 0.0,
            row_background: None,
            divider_width: 0.0,
            divider_color: None,
        }
    }
    /// Append one column definition.
    pub fn column(mut self, c: TableColumn) -> Self { self.columns.push(c); self }
    /// Append several column definitions.
    pub fn columns(mut self, cs: Vec<TableColumn>) -> Self { self.columns.extend(cs); self }
    /// Append a row of cell widgets (one per column, left to right).
    pub fn row(mut self, cells: Vec<BoxedWidget>) -> Self {
        self.row_lens.push(cells.len());
        self.cells.extend(cells);
        self
    }
    /// Horizontal gap between columns / vertical gap between rows.
    pub fn spacing(mut self, h: f32, v: f32) -> Self {
        self.h_spacing = h.max(0.0);
        self.v_spacing = v.max(0.0);
        self
    }
    /// Uniform padding inside every cell (logical px).
    pub fn cell_padding(mut self, p: f32) -> Self { self.cell_padding = p.max(0.0); self }
    /// Zebra striping: fill every odd row (1, 3, 5, …) with `c`.
    pub fn row_background(mut self, c: Color) -> Self { self.row_background = Some(c); self }
    /// Draw a hairline of `width` px between rows (centered in the
    /// vertical gap). Color defaults to the theme's `outline`.
    pub fn divider(mut self, width: f32) -> Self { self.divider_width = width.max(0.0); self }
    /// Override the divider hairline color.
    pub fn divider_color(mut self, c: Color) -> Self { self.divider_color = Some(c); self }

    /// Range of `self.cells` belonging to row `r`.
    fn row_range(&self, r: usize) -> std::ops::Range<usize> {
        let start: usize = self.row_lens[..r].iter().sum();
        start..start + self.row_lens[r]
    }

    /// Cell widget at (row, col), if that row has one.
    fn cell(&self, row: usize, col: usize) -> Option<&BoxedWidget> {
        let range = self.row_range(row);
        if col < self.row_lens[row] { self.cells.get(range.start + col) } else { None }
    }

    /// Resolve every column's width for `total_w` available px.
    ///
    /// fixed = as declared; auto = widest loose-measured cell + padding;
    /// flex = share of the leftover (intrinsic width when `total_w` is
    /// unbounded — documented on [`TableColumn::flex`]).
    fn resolve_columns(&self, ctx: &LayoutCtx, total_w: f32) -> Vec<f32> {
        let n = self.columns.len();
        let gaps = self.h_spacing * n.saturating_sub(1) as f32;
        let pad2 = self.cell_padding * 2.0;
        let bounded = total_w.is_finite();
        let measure_w = if bounded { total_w } else { f32::MAX };

        // Intrinsic width of column `i` = widest cell, loose-measured.
        let intrinsic = |i: usize| -> f32 {
            let mut w = 0.0f32;
            for row in 0..self.row_lens.len() {
                if let Some(cell) = self.cell(row, i) {
                    let s = cell.layout(&ctx.with_constraints(
                        Constraints::loose(measure_w, f32::INFINITY),
                    ));
                    w = w.max(s.width);
                }
            }
            w + pad2
        };

        let mut widths = vec![0.0f32; n];
        let mut flex_sum = 0.0f32;
        let mut used = 0.0f32;
        for (i, col) in self.columns.iter().enumerate() {
            match col.sizing {
                ColumnSizing::Fixed(px) => { widths[i] = px; used += px; }
                ColumnSizing::Auto => { widths[i] = intrinsic(i); used += widths[i]; }
                ColumnSizing::Flex(_) if !bounded => {
                    // No finite leftover to share — intrinsic fallback.
                    widths[i] = intrinsic(i);
                    used += widths[i];
                }
                ColumnSizing::Flex(f) => flex_sum += f,
            }
        }
        if bounded && flex_sum > 0.0 {
            let leftover = (total_w - used - gaps).max(0.0);
            for (i, col) in self.columns.iter().enumerate() {
                if let ColumnSizing::Flex(f) = col.sizing {
                    widths[i] = leftover * (f / flex_sum);
                }
            }
        }
        widths
    }

    /// Per-row heights at the given resolved column widths: the tallest
    /// cell of the row (measured at the column's content width) + padding.
    fn row_heights(&self, ctx: &LayoutCtx, widths: &[f32]) -> Vec<f32> {
        let pad2 = self.cell_padding * 2.0;
        (0..self.row_lens.len())
            .map(|row| {
                let mut h = 0.0f32;
                for (col, w) in widths.iter().enumerate() {
                    if let Some(cell) = self.cell(row, col) {
                        let s = cell.layout(&ctx.with_constraints(
                            Constraints::loose((w - pad2).max(0.0), f32::INFINITY),
                        ));
                        h = h.max(s.height);
                    }
                }
                h + pad2
            })
            .collect()
    }

    /// Content size for a given available width.
    fn content_size(&self, ctx: &LayoutCtx, total_w: f32) -> Size {
        let widths = self.resolve_columns(ctx, total_w);
        let heights = self.row_heights(ctx, &widths);
        let gaps_w = self.h_spacing * self.columns.len().saturating_sub(1) as f32;
        let gaps_h = self.v_spacing * heights.len().saturating_sub(1) as f32;
        Size {
            width: widths.iter().sum::<f32>() + gaps_w,
            height: heights.iter().sum::<f32>() + gaps_h,
        }
    }

    /// Whether any column is flex-sized (→ the table claims the full
    /// available width, like `Grid`/`Wrap` do).
    fn has_flex(&self) -> bool {
        self.columns.iter().any(|c| matches!(c.sizing, ColumnSizing::Flex(_)))
    }
}

impl Default for Table {
    fn default() -> Self { Self::new() }
}

impl Widget for Table {
    fn children(&self) -> Children<'_> { Children::Many(&self.cells) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = avail_w(ctx.constraints);
        let content = self.content_size(ctx, w);
        let width = if self.has_flex() && w.is_finite() { w } else { content.width };
        ctx.constraints.constrain(Size { width, height: content.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let divider = self
            .divider_color
            .unwrap_or_else(|| ctx.tc(ctx.theme.colors.outline));

        let r = ctx.rect;
        // Scoped so the immutable layout borrow ends before mutable painting.
        let (widths, heights) = {
            let lctx = ctx.layout_ctx(Constraints::loose(r.size.width, f32::INFINITY));
            let widths = self.resolve_columns(&lctx, r.size.width);
            let heights = self.row_heights(&lctx, &widths);
            (widths, heights)
        };

        let pad = self.cell_padding;
        let mut y = r.origin.y;
        for (row, row_h) in heights.iter().enumerate() {
            // Zebra stripe on odd rows.
            if row % 2 == 1 {
                if let Some(bg) = self.row_background {
                    ctx.fill_rect(
                        Rect {
                            origin: Point { x: r.origin.x, y },
                            size: Size { width: r.size.width, height: *row_h },
                        },
                        bg,
                    );
                }
            }

            let mut x = r.origin.x;
            for (col, w) in widths.iter().enumerate() {
                if let Some(cell) = self.cell(row, col) {
                    let content_w = (w - pad * 2.0).max(0.0);
                    let s = cell.layout(&ctx.layout_ctx(
                        Constraints::loose(content_w, f32::INFINITY),
                    ));
                    // Top-left alignment within the cell.
                    let rect = Rect {
                        origin: Point { x: x + pad, y: y + pad },
                        size: Size { width: s.width.min(content_w), height: s.height },
                    };
                    cell.paint(&mut ctx.child(rect));
                }
                x += w + self.h_spacing;
            }

            y += row_h;
            // Divider centered in the vertical gap after every row but the last.
            if row + 1 < heights.len() {
                if self.divider_width > 0.0 {
                    let dy = y + ((self.v_spacing - self.divider_width) / 2.0).max(0.0);
                    ctx.fill_rect(
                        Rect {
                            origin: Point { x: r.origin.x, y: dy },
                            size: Size { width: r.size.width, height: self.divider_width },
                        },
                        divider,
                    );
                }
                y += self.v_spacing;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A leaf reporting a fixed size regardless of constraints.
    struct FixedCell(f32, f32);
    impl Widget for FixedCell {
        fn layout(&self, _ctx: &LayoutCtx) -> Size {
            Size { width: self.0, height: self.1 }
        }
        fn paint(&self, _ctx: &mut PaintCtx) {}
    }

    fn boxed(w: f32, h: f32) -> BoxedWidget { Box::new(FixedCell(w, h)) }

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn fixed_auto_and_flex_columns_resolve_in_a_300px_width() {
        // fixed(100) + auto (widest cell 50) + flex(1) with 10px gaps:
        // leftover = 300 - 100 - 50 - 2*10 = 130.
        let table = Table::new()
            .column(TableColumn::fixed(100.0))
            .column(TableColumn::auto())
            .column(TableColumn::flex(1.0))
            .spacing(10.0, 0.0)
            .row(vec![boxed(40.0, 20.0), boxed(50.0, 30.0), boxed(10.0, 10.0)])
            .row(vec![boxed(80.0, 15.0), boxed(30.0, 12.0), boxed(10.0, 10.0)]);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        let widths = table.resolve_columns(&ctx, 300.0);
        assert_eq!(widths, vec![100.0, 50.0, 130.0]);
        // Flex column present → table claims the full available width.
        assert_eq!(table.layout(&ctx).width, 300.0);
    }

    #[test]
    fn two_flex_columns_share_leftover_by_factor() {
        // fixed(60) + flex(1) + flex(3), no gaps: leftover = 240 → 60/180.
        let table = Table::new()
            .column(TableColumn::fixed(60.0))
            .column(TableColumn::flex(1.0))
            .column(TableColumn::flex(3.0))
            .spacing(0.0, 0.0)
            .row(vec![boxed(10.0, 10.0), boxed(10.0, 10.0), boxed(10.0, 10.0)]);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        assert_eq!(table.resolve_columns(&ctx, 300.0), vec![60.0, 60.0, 180.0]);
    }

    #[test]
    fn row_height_is_the_tallest_cell_of_each_row() {
        let table = Table::new()
            .column(TableColumn::fixed(100.0))
            .column(TableColumn::fixed(100.0))
            .spacing(0.0, 10.0)
            .row(vec![boxed(40.0, 20.0), boxed(50.0, 44.0)])
            .row(vec![boxed(40.0, 16.0), boxed(50.0, 8.0)]);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        let heights = table.row_heights(&ctx, &[100.0, 100.0]);
        assert_eq!(heights, vec![44.0, 16.0]);
        // Total = 44 + 10 (v_spacing) + 16.
        assert_eq!(table.layout(&ctx).height, 70.0);
    }

    #[test]
    fn cell_padding_grows_auto_columns_and_row_heights() {
        let table = Table::new()
            .column(TableColumn::auto())
            .cell_padding(6.0)
            .row(vec![boxed(50.0, 20.0)]);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 1000.0), &font, &theme);
        assert_eq!(table.resolve_columns(&ctx, 300.0), vec![62.0]);
        assert_eq!(table.layout(&ctx).height, 32.0);
        // No flex column → content width, not the full 300.
        assert_eq!(table.layout(&ctx).width, 62.0);
    }
}
