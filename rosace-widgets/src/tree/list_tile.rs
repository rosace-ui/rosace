use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A standard list row: leading widget + title + subtitle + trailing widget.
pub struct ListTile {
    pub title: String,
    pub subtitle: Option<String>,
    pub leading: Option<BoxedWidget>,
    pub trailing: Option<BoxedWidget>,
    pub selected: bool,
    pub height: f32,
    pub padding_h: f32,
    /// `None` = read from the active theme's `typography.body_large`.
    pub title_size: Option<f32>,
    /// `None` = read from the active theme's `typography.body_medium`.
    pub subtitle_size: Option<f32>,
    /// `TRANSPARENT` (alpha 0) = use the active theme's `on_surface`.
    pub title_color: Color,
    press: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    /// `TRANSPARENT` (alpha 0) = use the active theme's `secondary`.
    pub subtitle_color: Color,
    pub bg: Color,
    /// `TRANSPARENT` (alpha 0) = use the active theme's `primary_container`.
    pub selected_bg: Color,
    /// `TRANSPARENT` (alpha 0) = use the active theme's `primary`.
    pub selected_accent: Color,
    pub divider: bool,
}

impl ListTile {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            leading: None,
            trailing: None,
            selected: false,
            height: 48.0,
            padding_h: 14.0,
            title_size: None,
            subtitle_size: None,
            title_color: Color::TRANSPARENT,
            subtitle_color: Color::TRANSPARENT,
            bg: Color::rgba(0, 0, 0, 0),
            selected_bg: Color::TRANSPARENT,
            selected_accent: Color::TRANSPARENT,
            divider: true,
            press: None,
        }
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Self { self.subtitle = Some(s.into()); self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn trailing(mut self, w: impl Widget + 'static) -> Self { self.trailing = Some(Box::new(w)); self }
    pub fn selected(mut self) -> Self { self.selected = true; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn no_divider(mut self) -> Self { self.divider = false; self }
    pub fn title_color(mut self, c: Color) -> Self { self.title_color = c; self }
    /// Overrides the theme's `body_large` for this row's title.
    pub fn title_size(mut self, s: f32) -> Self { self.title_size = Some(s); self }
    /// Overrides the theme's `body_medium` for this row's subtitle.
    pub fn subtitle_size(mut self, s: f32) -> Self { self.subtitle_size = Some(s); self }
    pub fn background(mut self, c: Color) -> Self { self.bg = c; self }

    /// Make the whole tile pressable.
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.press = Some(std::sync::Arc::new(f));
        self
    }

    /// Explicit size, else the theme's. Hardcoded 11/9 px before this —
    /// roughly two thirds of the platform body size, which is why lists read
    /// as noticeably smaller than native UI beside them. Material 3's list
    /// item uses body-large for the headline and body-medium for supporting
    /// text; these tokens are exactly that.
    pub fn resolved_title_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.title_size.unwrap_or(theme.typography.body_large.size)
    }

    pub fn resolved_subtitle_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.subtitle_size.unwrap_or(theme.typography.body_medium.size)
    }

    /// Greedy word-wrap, same primitive the `Text` widget uses.
    fn wrap_subtitle(
        &self,
        font: &rosace_render::FontCache,
        theme: &rosace_theme::ThemeData,
        sub: &str,
        max_w: f32,
    ) -> Vec<String> {
        let size = self.resolved_subtitle_size(theme);
        rosace_text::word_wrap(sub, max_w, |s| font.measure_text(s, size))
    }
}

impl Widget for ListTile {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let width = avail_w(constraints);
        // A subtitle wider than the row (leading/trailing/padding already
        // eat into it) used to just get clipped by the row's edge instead
        // of wrapping — `_text_w` sitting unused right below in `paint` was
        // the tell. Reserve the same leading/trailing budget `paint` uses
        // so the wrap width and the drawn width agree, then grow the row
        // past `self.height` when the wrapped subtitle needs more room.
        let height = match &self.subtitle {
            Some(sub) => {
                let lead_w = if self.leading.is_some() { 32.0 + 10.0 } else { 0.0 };
                let trail_w = if self.trailing.is_some() { 60.0 + self.padding_h } else { self.padding_h };
                let text_w = (width - self.padding_h - lead_w - trail_w).max(1.0);
                let sub_lines = self.wrap_subtitle(ctx.font, ctx.theme, sub, text_w).len().max(1);
                let line_h_title = ctx.font.line_height(self.resolved_title_size(ctx.theme));
                let line_h_sub = ctx.font.line_height(self.resolved_subtitle_size(ctx.theme));
                let content_h = line_h_title + 2.0 + line_h_sub * sub_lines as f32;
                self.height.max(content_h + 12.0)
            }
            None => self.height,
        };
        Size { width, height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let label = match &self.subtitle {
            Some(sub) => format!("{}, {}", self.title, sub),
            None => self.title.clone(),
        };
        // A row with a tap handler is a CONTROL, not just content. Declaring
        // it `ListItem` unconditionally left it announced but inert: platform
        // a11y layers key "can I activate this?" off the role, so VoiceOver
        // read the row and then offered no action (reported live on iOS,
        // 2026-08-09). Non-tappable rows stay `ListItem`, which is still the
        // right structural role for them.
        let role = if self.press.is_some() {
            rosace_core::Role::Button
        } else {
            rosace_core::Role::ListItem
        };
        ctx.semantics(super::SemanticsProps::new(role).label(label));
        if let Some(f) = &self.press {
            let f = f.clone();
            ctx.on_press(move || f());
            // Hover/press feedback, eased between three levels (D108 Phase
            // 26 Step 1) — idle, hover (matches the old flat 14-alpha wash),
            // press (double it).
            let target = if ctx.pressed() { 1.0 } else if ctx.hovered() { 0.5 } else { 0.0 };
            let emphasis = ctx.animate_to(target, 0.0);
            if emphasis > 0.0 {
                let a = (14.0 * emphasis * 2.0).min(255.0) as u8;
                ctx.fill_rect(ctx.rect, rosace_render::Color::rgba(255, 255, 255, a));
            }
        }
        let t = &ctx.theme.colors;
        let title_color = if self.title_color.a == 0 { ctx.tc(t.on_surface) } else { self.title_color };
        let subtitle_color = if self.subtitle_color.a == 0 { ctx.tc(t.secondary) } else { self.subtitle_color };
        let selected_bg = if self.selected_bg.a == 0 { ctx.tc(t.primary_container) } else { self.selected_bg };
        let selected_accent = if self.selected_accent.a == 0 { ctx.tc(t.primary) } else { self.selected_accent };
        let divider_color = ctx.tc(t.outline);

        let r = ctx.rect;
        let bg = if self.selected { selected_bg } else { self.bg };
        if bg.a > 0 { ctx.fill_rect(r, bg); }

        if self.selected {
            ctx.fill_rect(Rect {
                origin: r.origin,
                size: Size { width: 2.5, height: r.size.height },
            }, selected_accent);
        }

        let mut x = r.origin.x + self.padding_h;

        // Leading
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(32.0, r.size.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect {
                origin: Point { x, y: ly },
                size: ls,
            }));
            x += ls.width + 10.0;
        }

        // Trailing
        let trailing_w = if let Some(trail) = &self.trailing {
            let ts = trail.layout(&ctx.layout_ctx(Constraints::loose(60.0, r.size.height)));
            let ty = r.origin.y + (r.size.height - ts.height) / 2.0;
            let tx = r.origin.x + r.size.width - self.padding_h - ts.width;
            trail.paint(&mut ctx.child(Rect { origin: Point { x: tx, y: ty }, size: ts }));
            ts.width + self.padding_h
        } else { self.padding_h };

        // Title + subtitle
        let text_w = r.size.width - x + r.origin.x - trailing_w;
        let line_h_title = ctx.font.line_height(self.resolved_title_size(&ctx.theme));
        let line_h_sub = ctx.font.line_height(self.resolved_subtitle_size(&ctx.theme));
        let sub_lines = self.subtitle.as_ref()
            .map(|sub| self.wrap_subtitle(ctx.font, &ctx.theme, sub, text_w.max(1.0)))
            .unwrap_or_default();
        let total_text_h = if sub_lines.is_empty() {
            line_h_title
        } else {
            line_h_title + 2.0 + line_h_sub * sub_lines.len() as f32
        };
        let text_y = r.origin.y + (r.size.height - total_text_h) / 2.0;

        ctx.draw_text_at(&self.title, Point { x, y: text_y }, title_color, self.resolved_title_size(&ctx.theme));

        for (i, line) in sub_lines.iter().enumerate() {
            let sub_y = text_y + line_h_title + 2.0 + line_h_sub * i as f32;
            ctx.draw_text_at(line, Point { x, y: sub_y }, subtitle_color, self.resolved_subtitle_size(&ctx.theme));
        }

        if self.divider {
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x + self.padding_h, y: r.origin.y + r.size.height - 1.0 },
                size: Size { width: r.size.width - self.padding_h, height: 1.0 },
            }, divider_color);
        }
    }
}

