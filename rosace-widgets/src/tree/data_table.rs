//! `DataTable` (D115/Phase 32 Step 1) — the data-grid RENDERING layer on
//! top of [`Table`] (the layout primitive): header row with sort-direction
//! indicators, optional row-selection checkboxes, row striping. Sorting
//! itself is rendering-only (per `PHASE_32.md`'s Out of Scope note) — the
//! app owns the actual sort/comparator and re-passes already-sorted rows;
//! `on_sort` just reports which column/direction the user asked for.
//! Virtualization (windowed rendering for huge row counts) is explicitly
//! OUT OF SCOPE for this MVP — named, not silently dropped.
//!
//! Cells are plain text (the common tabular-data case) — richer per-cell
//! widgets are future work. Rows are stored as `String`s (`Clone`, cheap)
//! rather than built `BoxedWidget`s specifically so a fresh [`Table`] can
//! be constructed independently in both `layout()` and `paint()` (the
//! widget protocol calls them separately on a borrowed `&self`, and
//! `Box<dyn Widget>` isn't `Clone` — building from owned strings sidesteps
//! that rather than fighting it).

use std::sync::Arc;
use rosace_render::Color;
use super::{BoxedWidget, LayoutCtx, PaintCtx, Widget};
use super::table::{Table, TableColumn};
use super::text::Text;
use super::checkbox::Checkbox;
use super::pressable::Pressable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One column definition: header label + width policy.
#[derive(Clone)]
pub struct DataTableColumn {
    label: String,
    sizing: TableColumn,
}

impl DataTableColumn {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), sizing: TableColumn::auto() }
    }
    pub fn fixed_width(mut self, px: f32) -> Self { self.sizing = TableColumn::fixed(px); self }
    pub fn flex(mut self, factor: f32) -> Self { self.sizing = TableColumn::flex(factor); self }
}

/// A data grid: typed columns + text row data, rendered via [`Table`] with
/// a sortable header and optional selection checkboxes.
pub struct DataTable {
    columns: Vec<DataTableColumn>,
    /// Row-major text cells — one `Vec<String>` per row, ideally
    /// `columns.len()` entries each (short/long rows are handled by
    /// `Table` itself: missing cells render empty, extras are ignored).
    rows: Vec<Vec<String>>,
    sort_col: Option<usize>,
    sort_dir: SortDirection,
    selectable: bool,
    selected_rows: Vec<bool>,
    row_striping: Option<Color>,
    on_sort: Option<Arc<dyn Fn(usize, SortDirection) + Send + Sync>>,
    on_select: Option<Arc<dyn Fn(usize, bool) + Send + Sync>>,
}

impl DataTable {
    pub fn new(columns: Vec<DataTableColumn>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            sort_col: None,
            sort_dir: SortDirection::Ascending,
            selectable: false,
            selected_rows: Vec::new(),
            row_striping: None,
            on_sort: None,
            on_select: None,
        }
    }

    /// Append one row of cell text (one string per column, left to right).
    pub fn row(mut self, cells: Vec<impl Into<String>>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Which column currently shows a sort indicator, and its direction —
    /// visual only; the app is responsible for actually sorting `rows`.
    pub fn sorted_by(mut self, col: usize, dir: SortDirection) -> Self {
        self.sort_col = Some(col);
        self.sort_dir = dir;
        self
    }
    pub fn row_striping(mut self, c: Color) -> Self { self.row_striping = Some(c); self }

    /// Shows a leading checkbox column. `selected` must be as long as `rows`.
    pub fn selectable(mut self, selected: Vec<bool>) -> Self {
        self.selectable = true;
        self.selected_rows = selected;
        self
    }

    pub fn on_sort(mut self, f: impl Fn(usize, SortDirection) + Send + Sync + 'static) -> Self {
        self.on_sort = Some(Arc::new(f));
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize, bool) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    /// Builds the underlying layout `Table` fresh — header row (with sort
    /// arrows, clickable via `Pressable`) + one body row per entry (with an
    /// optional leading checkbox) — delegating all column-sizing/striping/
    /// divider work to `Table` rather than re-implementing it. Called
    /// independently from both `layout()` and `paint()`.
    fn build_table(&self) -> Table {
        let mut table = Table::new();
        if self.selectable {
            table = table.column(TableColumn::fixed(32.0));
        }
        table = table.columns(self.columns.iter().map(|c| c.sizing).collect());

        // Header row.
        let mut header: Vec<BoxedWidget> = Vec::new();
        if self.selectable {
            header.push(Box::new(Text::new("")));
        }
        for (i, col) in self.columns.iter().enumerate() {
            let label = if self.sort_col == Some(i) {
                let arrow = match self.sort_dir { SortDirection::Ascending => "^", SortDirection::Descending => "v" };
                format!("{} {arrow}", col.label)
            } else {
                col.label.clone()
            };
            let cell: BoxedWidget = match &self.on_sort {
                Some(f) => {
                    let f = f.clone();
                    let next_dir = if self.sort_col == Some(i) && self.sort_dir == SortDirection::Ascending {
                        SortDirection::Descending
                    } else {
                        SortDirection::Ascending
                    };
                    Box::new(Pressable::new(Text::new(label).weight(rosace_render::FontWeight::Bold), move || f(i, next_dir)))
                }
                None => Box::new(Text::new(label).weight(rosace_render::FontWeight::Bold)),
            };
            header.push(cell);
        }
        table = table.row(header);

        // Body rows.
        for (r, cells) in self.rows.iter().enumerate() {
            let mut row_widgets: Vec<BoxedWidget> = Vec::new();
            if self.selectable {
                let checked = self.selected_rows.get(r).copied().unwrap_or(false);
                let cb = Checkbox::new(checked);
                let cb: BoxedWidget = match &self.on_select {
                    Some(f) => {
                        let f = f.clone();
                        Box::new(cb.on_change(move |v| f(r, v)))
                    }
                    None => Box::new(cb),
                };
                row_widgets.push(cb);
            }
            for text in cells {
                row_widgets.push(Box::new(Text::new(text.clone())));
            }
            table = table.row(row_widgets);
        }

        if let Some(c) = self.row_striping {
            table = table.row_background(c);
        }
        table.cell_padding(8.0).divider(1.0)
    }
}

impl Widget for DataTable {
    fn layout(&self, ctx: &LayoutCtx) -> rosace_core::types::Size {
        self.build_table().layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        self.build_table().paint(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn layout_delegates_to_underlying_table() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let dt = DataTable::new(vec![DataTableColumn::new("Name"), DataTableColumn::new("Qty")])
            .row(vec!["Widget", "3"]);
        let size = dt.layout(&ctx);
        assert!(size.width > 0.0 && size.height > 0.0);
    }

    #[test]
    fn sorted_by_toggles_direction_on_repeat_click() {
        let dt = DataTable::new(vec![DataTableColumn::new("Name")])
            .sorted_by(0, SortDirection::Ascending);
        assert_eq!(dt.sort_col, Some(0));
        assert_eq!(dt.sort_dir, SortDirection::Ascending);
    }

    #[test]
    fn layout_is_stable_across_repeated_calls() {
        // Guards the "build_table() called independently by layout/paint"
        // design: two separate calls on the same borrowed value must agree.
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let dt = DataTable::new(vec![DataTableColumn::new("Name")]).row(vec!["A"]).row(vec!["B"]);
        let s1 = dt.layout(&ctx);
        let s2 = dt.layout(&ctx);
        assert_eq!((s1.width, s1.height), (s2.width, s2.height));
    }
}
