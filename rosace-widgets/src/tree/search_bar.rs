//! `SearchBar` (D115/Phase 32 Step 1) — a search field composite:
//! leading search icon, a real Phase-28 `TextInput` (full keyboard/IME
//! editing for free), and an optional clear button when non-empty.
//!
//! Controlled like `TextInput` itself: pass `.value()` + `.on_change()`
//! wired to your atom; `.on_clear()` fires from the × button.

use std::sync::{Arc, OnceLock};

use rosace_core::types::Size;

use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

pub struct SearchBar {
    value: String,
    placeholder: String,
    width: Option<f32>,
    height: f32,
    font_size: f32,
    on_change: Option<Arc<dyn Fn(String) + Send + Sync>>,
    on_clear: Option<Arc<dyn Fn() + Send + Sync>>,
    inner: OnceLock<BoxedWidget>,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: "Search\u{2026}".to_string(),
            width: None,
            height: 34.0,
            font_size: 13.0,
            on_change: None,
            on_clear: None,
            inner: OnceLock::new(),
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
    /// Shows the trailing × button whenever the value is non-empty.
    pub fn on_clear(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_clear = Some(Arc::new(f));
        self
    }

    /// The composed row, built once per widget instance (widgets are
    /// rebuilt every frame by `build()`, so this never goes stale).
    fn inner(&self) -> &BoxedWidget {
        self.inner.get_or_init(|| {
            let field_w = self.width.unwrap_or(260.0);
            let mut input = super::TextInput::new()
                .value(self.value.clone())
                .placeholder(self.placeholder.clone())
                .width(field_w)
                .height(self.height);
            if let Some(f) = &self.on_change {
                let f = Arc::clone(f);
                input = input.on_change(move |v| f(v));
            }

            let mut row = super::Row::new()
                .spacing(8.0)
                .child(super::Icon::new(super::IconKind::Search).size(16.0))
                .child(input);

            if let Some(clear) = &self.on_clear {
                if !self.value.is_empty() {
                    let clear = Arc::clone(clear);
                    row = row.child(super::Pressable::new(
                        super::Text::new("\u{00d7}").size(self.font_size + 3.0),
                        move || clear(),
                    ));
                }
            }
            Box::new(row)
        })
    }
}

impl Default for SearchBar {
    fn default() -> Self { Self::new() }
}

impl Widget for SearchBar {
    fn children(&self) -> Children<'_> {
        Children::One(self.inner().as_ref())
    }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        self.inner().layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        self.inner().paint(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn search_bar_lays_out_wider_than_its_input_by_the_icon_gutter() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(500.0, 60.0), &font, &theme);
        let sb = SearchBar::new().width(200.0);
        let size = sb.layout(&ctx);
        assert!(size.width > 200.0, "icon + spacing must add to the input width, got {}", size.width);
        assert!(size.height >= 34.0);
    }
}
