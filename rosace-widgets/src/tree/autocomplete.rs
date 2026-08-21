//! `Autocomplete` — a text field with a filtered suggestion dropdown, the
//! typeahead/combobox pattern `SearchBar` and `Dropdown` don't individually
//! cover (one is a plain text field, the other has no live-typed query).
//!
//! Built the same way `SearchBar` is (a preset over [`super::TextInput`])
//! plus the same overlay mechanism [`super::Dropdown`] uses for its option
//! list ([`super::overlay::push_overlay`]) — no new framework primitive,
//! just composing the two existing patterns. `open` auto-manages itself
//! from typing (non-empty query → open) via a wrapped `on_change`, so
//! callers only need to supply an initially-`false` atom, same as
//! `Dropdown` — not a fully separate state contract to learn.

use std::sync::Arc;
use rosace_core::types::{Point, Size};
use rosace_render::Color;

use super::menu::Menu;
use super::overlay::{FocusBehavior, InputBehavior, LayerPosition, ScrimConfig};
use super::{Children, LayoutCtx, PaintCtx, Widget};

/// A text field with a live-filtered suggestion dropdown below it.
pub struct Autocomplete {
    value: String,
    placeholder: String,
    options: Vec<String>,
    open: bool,
    on_open_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    width: Option<f32>,
    height: f32,
    max_visible: usize,
    on_change: Option<Arc<dyn Fn(String) + Send + Sync>>,
    on_select: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl Autocomplete {
    /// `options` is the full candidate list to filter against; `open` is an
    /// initially-`false` atom this widget manages as the user types.
    pub fn new(options: Vec<impl Into<String>>, open: bool) -> Self {
        Self {
            value: String::new(),
            placeholder: "Search\u{2026}".to_string(),
            options: options.into_iter().map(Into::into).collect(),
            open,
            on_open_change: None,
            width: None,
            height: 36.0,
            max_visible: 6,
            on_change: None,
            on_select: None,
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    /// Cap on how many matches show at once (default 6).
    pub fn max_visible(mut self, n: usize) -> Self { self.max_visible = n.max(1); self }
    /// Fired on every keystroke with the raw field text.
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f)); self
    }
    /// Fired once when a suggestion is tapped, with the chosen option.
    pub fn on_select(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f)); self
    }
    /// Called with the NEW open state when the suggestion list opens or closes.
    pub fn on_open_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_open_change = Some(Arc::new(f)); self
    }

    /// Matches for the current value — case-insensitive substring, capped
    /// at `max_visible`. Standalone so it's unit-testable without a paint
    /// context.
    fn matches(&self) -> Vec<&String> {
        let q = self.value.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.options
            .iter()
            .filter(|o| o.to_lowercase().contains(&q))
            .take(self.max_visible)
            .collect()
    }

    /// The underlying field. Built fresh per call (like `Dropdown` rebuilds
    /// its `Menu` every paint) rather than cached — `on_change` needs to
    /// close over this call's `self.open`/`self.on_change`, which change
    /// across rebuilds.
    fn input(&self) -> super::TextInput {
        let mut input = super::TextInput::new()
            .value(self.value.clone())
            .placeholder(self.placeholder.clone())
            .height(self.height)
            .leading(super::Icon::new(super::IconKind::Search).size(18.0));
        if let Some(w) = self.width {
            input = input.width(w);
        }
        let open_cb = self.on_open_change.clone();
        let user_on_change = self.on_change.clone();
        input = input.on_change(move |v| {
            if let Some(f) = &open_cb { f(!v.trim().is_empty()); }
            if let Some(f) = &user_on_change {
                f(v);
            }
        });
        input
    }
}

impl Widget for Autocomplete {
    fn children(&self) -> Children<'_> { Children::None }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        self.input().layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        self.input().paint(ctx);

        let filtered = self.matches();
        if self.open && !filtered.is_empty() {
            let pos = Point { x: r.origin.x, y: r.origin.y + r.size.height + 4.0 };
            let mut menu = Menu::new().min_width(self.width.unwrap_or(r.size.width));
            for opt in filtered {
                let chosen = opt.clone();
                let close = self.on_open_change.clone();
                let cb = self.on_select.clone();
                menu = menu.item(opt.clone(), move || {
                    if let Some(f) = &close { f(false); }
                    if let Some(cb) = &cb {
                        cb(chosen.clone());
                    }
                });
            }
            let close2 = self.on_open_change.clone();
            ctx.promote_at(
                LayerPosition::Absolute(pos),
                &menu,
                super::PromoteOpts {
                    scrim: Some(ScrimConfig {
                        color: Color::TRANSPARENT,
                        on_tap: Some(Arc::new(move || { if let Some(f) = &close2 { f(false); } })),
                        // Own field's rect — typing/clicking there is
                        // handled by the field itself, not outside-tap
                        // dismiss (same reasoning as `Dropdown`'s trigger).
                        exclude_rect: Some(r),
                    }),
                    input: InputBehavior::PassThrough,
                    focus: FocusBehavior::PassThrough,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;


    #[test]
    fn matches_filters_case_insensitively() {
        let ac = Autocomplete::new(vec!["Apple", "Banana", "apricot"], false)
            .value("ap");
        let m: Vec<&str> = ac.matches().iter().map(|s| s.as_str()).collect();
        assert_eq!(m, vec!["Apple", "apricot"]);
    }

    #[test]
    fn empty_query_has_no_matches() {
        let ac = Autocomplete::new(vec!["Apple", "Banana"], false);
        assert!(ac.matches().is_empty());
    }

    #[test]
    fn respects_max_visible() {
        let ac = Autocomplete::new(vec!["a1", "a2", "a3", "a4"], false)
            .value("a")
            .max_visible(2);
        assert_eq!(ac.matches().len(), 2);
    }

    #[test]
    fn layout_matches_the_underlying_field() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(500.0, 60.0), &font, &theme);
        let ac = Autocomplete::new(vec!["A", "B"], false).width(240.0);
        assert_eq!(ac.layout(&ctx).width, 240.0);
    }
}
