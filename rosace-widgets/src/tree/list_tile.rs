use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{EdgeInsets, Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A standard list row: leading widget + title + subtitle + trailing widget.
pub struct ListTile {
    pub title: String,
    pub subtitle: Option<String>,
    pub leading: Option<BoxedWidget>,
    pub trailing: Option<BoxedWidget>,
    pub selected: bool,
    pub height: f32,
    /// Horizontal inset. Kept as a plain field for source compatibility;
    /// prefer [`ListTile::padding`], which also controls the vertical inset.
    pub padding_h: f32,
    /// `None` = `padding_h` on both sides and none vertically — the
    /// long-standing behaviour. A full-bleed row inside a list rarely wants
    /// vertical padding, but a standalone tile often does.
    padding: Option<EdgeInsets>,
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
            padding: None,
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
    /// Inset around the row's content. Was reachable only by assigning the
    /// public `padding_h` field, which breaks the builder chain and could
    /// not express a vertical inset at all.
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = Some(p); self }
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

impl ListTile {
    /// The effective inset: an explicit `.padding(..)`, else the legacy
    /// horizontal-only behaviour.
    fn insets(&self) -> EdgeInsets {
        self.padding.unwrap_or(EdgeInsets::symmetric(self.padding_h, 0.0))
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
                let pad = self.insets();
                let trail_w = if self.trailing.is_some() { 60.0 + pad.right } else { pad.right };
                let text_w = (width - pad.left - lead_w - trail_w).max(1.0);
                let sub_lines = self.wrap_subtitle(ctx.font, ctx.theme, sub, text_w).len().max(1);
                let line_h_title = ctx.font.line_height(self.resolved_title_size(ctx.theme));
                let line_h_sub = ctx.font.line_height(self.resolved_subtitle_size(ctx.theme));
                let content_h = line_h_title + 2.0 + line_h_sub * sub_lines as f32;
                self.height.max(content_h + 12.0)
            }
            None => {
                // The subtitle branch above already grows with scaled text;
                // this one returned the designed height flat, so a
                // title-only row clipped its title at raised OS text sizes
                // while an otherwise identical row WITH a subtitle did not.
                let line_h = ctx.font.line_height(self.resolved_title_size(ctx.theme));
                self.height.max(line_h + 12.0)
            }
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
        // `selected` is painted (a tinted background and an accent bar) and
        // was never announced, so the highlight was visual-only.
        let mut sem = super::SemanticsProps::new(role).label(label);
        if self.selected { sem = sem.value("selected"); }
        ctx.semantics(sem);
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
                // A white wash is invisible on a light theme. Every OTHER
                // colour in this file already resolves from a token.
                let w = ctx.tc(ctx.theme.colors.on_surface);
                ctx.fill_rect(ctx.rect, rosace_render::Color::rgba(w.r, w.g, w.b, a));
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

        let pad = self.insets();
        let mut x = r.origin.x + pad.left;

        // Leading
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(32.0, r.size.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            ctx.paint_child(Rect {
                origin: Point { x, y: ly },
                size: ls,
            }, &*lead);
            x += ls.width + 10.0;
        }

        // Trailing
        let trailing_w = if let Some(trail) = &self.trailing {
            let ts = trail.layout(&ctx.layout_ctx(Constraints::loose(60.0, r.size.height)));
            let ty = r.origin.y + (r.size.height - ts.height) / 2.0;
            let tx = r.origin.x + r.size.width - pad.right - ts.width;
            ctx.paint_child(Rect { origin: Point { x: tx, y: ty }, size: ts }, &*trail);
            ts.width + pad.right
        } else { pad.right };

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
                origin: Point { x: r.origin.x + pad.left, y: r.origin.y + r.size.height - 1.0 },
                size: Size { width: r.size.width - pad.left, height: 1.0 },
            }, divider_color);
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    /// `.padding(..)` must move the row's content, and the default must
    /// reproduce the long-standing `padding_h`-only behaviour.
    ///
    /// Asserted on the painted TEXT origin rather than the layout size: a
    /// ListTile's height is a fixed row, so a padding builder that did
    /// nothing would still return the same Size and look correct.
    #[test]
    fn padding_moves_the_content_and_defaults_to_the_legacy_inset() {
        use rosace_render::{DrawCommand, FontCache, PictureRecorder};
        use crate::tree::RenderTree;
        use std::{cell::RefCell, rc::Rc};

        fn first_text_x(tile: ListTile) -> f32 {
            let font = FontCache::embedded();
            let mut rec = PictureRecorder::new();
            {
                let mut ctx = PaintCtx::root(
                    &mut rec,
                    Rect { origin: Point { x: 0.0, y: 0.0 },
                           size: Size { width: 300.0, height: 60.0 } },
                    &font,
                    rosace_theme::built_in::dark_theme(),
                    Rc::new(RefCell::new(RenderTree::new())),
                );
                tile.paint(&mut ctx);
            }
            rec.finish().commands.iter().find_map(|c| match c {
                DrawCommand::DrawText { origin, .. } => Some(origin.x),
                _ => None,
            }).expect("the title must be drawn")
        }

        let default_x = first_text_x(ListTile::new("Title"));
        assert_eq!(default_x, 14.0, "default is the legacy padding_h");

        let padded_x = first_text_x(ListTile::new("Title").padding(EdgeInsets::all(32.0)));
        assert_eq!(padded_x, 32.0, "an explicit inset must move the title");
    }

    /// A title-only row must grow with scaled text, like the subtitle
    /// branch already did.
    ///
    /// The two branches diverged: with a subtitle the height was
    /// `max(designed, content)`, without one it returned the designed
    /// height flat. So an otherwise identical row clipped its title at
    /// raised OS text sizes purely because it had no subtitle — asserted
    /// with a large title size rather than by moving `text_scale`, which is
    /// a process-global the parallel suite shares.
    #[test]
    fn a_title_only_row_grows_with_its_title_like_the_subtitle_branch() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 600.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let normal = ListTile::new("Title").layout(&ctx).height;
        let big = ListTile::new("Title").title_size(40.0).layout(&ctx).height;
        assert!(big > normal,
            "a title-only row did not grow for larger type: {normal} -> {big}");
        assert!(big >= font.line_height(40.0),
            "the row must clear its own line box");
    }

}
