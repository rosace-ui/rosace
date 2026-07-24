//! `SearchBar` — there is no separate "search bar" widget. A search field is
//! just a [`super::TextInput`] with a **leading** search icon (and an optional
//! **trailing** clear ×), using `TextInput`'s adornment API. This type is a
//! thin, convenient preset over exactly that — the icon lives *inside* the
//! field (one pill), not beside it. The same adornments give you password
//! fields (`.trailing(eye).on_trailing(toggle)`), prefixes (`$`), etc.

use std::sync::{Arc, OnceLock};

use rosace_core::types::Size;

use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

pub struct SearchBar {
    value: String,
    placeholder: String,
    width: Option<f32>,
    height: f32,
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
            height: 36.0,
            on_change: None,
            on_clear: None,
            inner: OnceLock::new(),
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f)); self
    }
    /// Shows a trailing clear (×) whenever the value is non-empty.
    pub fn on_clear(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_clear = Some(Arc::new(f)); self
    }

    fn inner(&self) -> &BoxedWidget {
        self.inner.get_or_init(|| {
            let mut input = super::TextInput::new()
                .value(self.value.clone())
                .placeholder(self.placeholder.clone())
                .height(self.height)
                .leading(super::Icon::new(super::IconKind::Search).size(18.0));
            if let Some(w) = self.width { input = input.width(w); }
            if let Some(f) = &self.on_change {
                let f = Arc::clone(f);
                input = input.on_change(move |v| f(v));
            }
            if let Some(clear) = &self.on_clear {
                if !self.value.is_empty() {
                    let clear = Arc::clone(clear);
                    input = input
                        .trailing(super::Text::new("\u{00d7}").size(16.0))
                        .on_trailing(move || clear());
                }
            }
            Box::new(input)
        })
    }
}

impl Default for SearchBar {
    fn default() -> Self { Self::new() }
}

impl Widget for SearchBar {
    fn children(&self) -> Children<'_> { Children::One(self.inner().as_ref()) }
    fn layout(&self, ctx: &LayoutCtx) -> Size { self.inner().layout(ctx) }
    fn paint(&self, ctx: &mut PaintCtx) { self.inner().paint(ctx); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn search_bar_is_a_text_input_with_a_leading_icon() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(500.0, 60.0), &font, &theme);
        let sb = SearchBar::new().width(200.0);
        // Delegates to a TextInput of the requested width (adornments are
        // inside the field, so the outer size is the field's size).
        assert_eq!(sb.layout(&ctx).width, 200.0);
    }
}
