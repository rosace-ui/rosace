//! `TabView` (shows one of N children) and `Tabs` (an interactive `TabBar` over
//! a `TabView`). Selection state is external (a `usize` + `on_change`), matching
//! `SegmentedControl` — the parent holds it in `ctx.state`, so tabs stay
//! stateless value types. The tab *strip* is the interactive [`TabBar`] in
//! `tab.rs`; these compose it with switchable content.
//!
//! General-purpose; also the substrate for the in-app DevTools panel
//! (Elements · Network · Logs …).

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};
use super::tab::{Tab, TabBar};

type OnChange = Arc<dyn Fn(usize) + Send + Sync>;

/// Shows exactly one of its children — the one at `selected`. A thin wrapper:
/// it declares the selected child as its single child, so the default layout
/// and paint fill this widget's rect with it.
pub struct TabView {
    children: Vec<BoxedWidget>,
    selected: usize,
}

impl TabView {
    pub fn new(children: Vec<BoxedWidget>, selected: usize) -> Self {
        Self { children, selected }
    }
}

impl Widget for TabView {
    fn children(&self) -> Children<'_> {
        match self.children.get(self.selected) {
            Some(c) => Children::One(&**c),
            None => Children::None,
        }
    }
}

/// An interactive [`TabBar`] over a [`TabView`]: give it labeled content and the
/// current selection; it lays the bar across the top and the active content
/// below. Selection is external (`selected` + `on_change`) — the parent keeps
/// it in `ctx.state`.
///
/// ```ignore
/// let tab = ctx.state(0usize);
/// let t = tab.clone();
/// Tabs::new(tab.get(), move |i| t.set(i))
///     .tab("Elements", elements_view())
///     .tab("Network",  network_view())
/// ```
pub struct Tabs {
    labels: Vec<String>,
    contents: Vec<BoxedWidget>,
    selected: usize,
    bar_height: f32,
    on_change: Option<OnChange>,
    // Bar customization, all forwarded to the internal TabBar (theme-defaulted).
    background: Option<Color>,
    active: Option<Color>,
    inactive: Option<Color>,
    indicator: Option<Color>,
    font_size: Option<f32>,
    animated: bool,
}

impl Tabs {
    pub fn new(selected: usize, on_change: impl Fn(usize) + Send + Sync + 'static) -> Self {
        Self {
            labels: Vec::new(),
            contents: Vec::new(),
            selected,
            bar_height: 40.0,
            on_change: Some(Arc::new(on_change)),
            background: None, active: None, inactive: None, indicator: None, font_size: None, animated: true,
        }
    }
    /// Non-interactive tabs (no selection callback).
    pub fn readonly(selected: usize) -> Self {
        Self {
            labels: Vec::new(), contents: Vec::new(), selected, bar_height: 40.0, on_change: None,
            background: None, active: None, inactive: None, indicator: None, font_size: None, animated: true,
        }
    }
    pub fn tab(mut self, label: impl Into<String>, content: impl Widget + 'static) -> Self {
        self.labels.push(label.into());
        self.contents.push(Box::new(content));
        self
    }
    pub fn bar_height(mut self, h: f32) -> Self { self.bar_height = h; self }
    /// Bar background — theme `surface` if unset.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Selected-tab label color — theme `on_surface` if unset.
    pub fn active_color(mut self, c: Color) -> Self { self.active = Some(c); self }
    /// Unselected-tab label color — a muted default if unset.
    pub fn inactive_color(mut self, c: Color) -> Self { self.inactive = Some(c); self }
    /// Sliding-underline color — theme `primary` if unset.
    pub fn indicator_color(mut self, c: Color) -> Self { self.indicator = Some(c); self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = Some(s); self }
    /// Turn the sliding-underline animation off (on by default).
    pub fn animated(mut self, on: bool) -> Self { self.animated = on; self }
}

impl Widget for Tabs {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = ctx.constraints.max_width_f32();
        let h = ctx.constraints.max_height_f32();
        let w = if w.is_finite() { w } else { 0.0 };
        let h = if h.is_finite() { h } else { self.bar_height };
        ctx.constraints.constrain(Size { width: w, height: h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let bar_rect = Rect { origin: r.origin, size: Size { width: r.size.width, height: self.bar_height } };
        let content_rect = Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + self.bar_height },
            size: Size { width: r.size.width, height: (r.size.height - self.bar_height).max(0.0) },
        };

        // Build the interactive tab strip (cheap value type, rebuilt each paint).
        let mut bar = TabBar::new().selected(self.selected).height(self.bar_height).animated(self.animated);
        for label in &self.labels {
            bar = bar.tab(Tab::new(label));
        }
        if let Some(c) = self.background { bar = bar.background(c); }
        if let Some(c) = self.active { bar = bar.active_color(c); }
        if let Some(c) = self.inactive { bar = bar.inactive_color(c); }
        if let Some(c) = self.indicator { bar = bar.indicator_color(c); }
        if let Some(s) = self.font_size { bar = bar.font_size(s); }
        if let Some(cb) = &self.on_change {
            let cb = cb.clone();
            bar = bar.on_change(move |i| cb(i));
        }
        bar.paint(&mut ctx.child(bar_rect));

        // The active content.
        if let Some(content) = self.contents.get(self.selected) {
            content.paint(&mut ctx.child(content_rect));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Text;

    #[test]
    fn tabview_declares_the_selected_child() {
        let tv = TabView::new(vec![Box::new(Text::new("a")), Box::new(Text::new("b"))], 1);
        assert!(matches!(tv.children(), Children::One(_)));
    }

    #[test]
    fn tabview_out_of_range_is_empty() {
        let tv = TabView::new(vec![Box::new(Text::new("a"))], 9);
        assert!(matches!(tv.children(), Children::None));
    }

    #[test]
    fn tabs_builder_collects_labels_and_content() {
        let t = Tabs::readonly(0)
            .tab("One", Text::new("1"))
            .tab("Two", Text::new("2"));
        assert_eq!(t.labels, vec!["One", "Two"]);
        assert_eq!(t.contents.len(), 2);
    }
}
