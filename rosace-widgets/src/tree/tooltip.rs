use rosace_core::types::{Point, Size};
use rosace_render::Color;

use super::overlay::{FocusBehavior, InputBehavior, LayerPosition, OverlayEntry};
use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// Theme-driven tooltip appearance (D115/Phase 32). Set once on the theme
/// via [`rosace_theme::ThemeData::with_ext`] and every `.tooltip(..)` /
/// [`Tooltip`] in the app picks it up — shape, colors, font, padding all
/// come from here, with a sensible default when the theme sets none. Same
/// type-keyed extension mechanism `CursorStyle` uses (D105).
#[derive(Debug, Clone, Copy)]
pub struct TooltipStyle {
    pub background: Color,
    pub text_color: Color,
    pub radius: f32,
    pub font_size: f32,
    /// Horizontal padding inside the bubble; vertical padding is derived
    /// from the font size so the label always sits centered.
    pub pad_h: f32,
    /// Drop-shadow strength; `0.0` disables it.
    pub elevation: f32,
}

impl Default for TooltipStyle {
    fn default() -> Self {
        Self {
            background: Color::rgba(40, 42, 58, 245),
            text_color: Color::rgb(228, 230, 244),
            radius: 6.0,
            font_size: 12.0,
            pad_h: 10.0,
            elevation: 1.0,
        }
    }
}

impl TooltipStyle {
    /// Resolve from the active theme's extension, else the default.
    fn resolve(theme: &rosace_theme::ThemeData) -> Self {
        theme.ext::<TooltipStyle>().copied().unwrap_or_default()
    }
}

/// Wraps a child and shows a floating label while the pointer hovers it.
///
/// Prefer the ergonomic `widget.tooltip("text")` ([`super::WidgetExt`])
/// over constructing this directly — same result, no wrapping. Styling is
/// theme-driven ([`TooltipStyle`]); an explicit `.style(..)` overrides it
/// per-tooltip.
pub struct Tooltip {
    label: String,
    style: Option<TooltipStyle>,
    child: BoxedWidget,
}

impl Tooltip {
    pub fn new(label: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self { label: label.into(), style: None, child: Box::new(child) }
    }
    /// Per-tooltip style override (otherwise the theme's `TooltipStyle`).
    pub fn style(mut self, style: TooltipStyle) -> Self {
        self.style = Some(style);
        self
    }
    /// Convenience: override just the font size on the resolved style.
    pub fn font_size(mut self, s: f32) -> Self {
        let mut st = self.style.unwrap_or_default();
        st.font_size = s;
        self.style = Some(st);
        self
    }
}

impl Widget for Tooltip {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
        ctx.hoverable();
        if ctx.hovered() {
            let style = self.style.unwrap_or_else(|| TooltipStyle::resolve(&ctx.theme));
            let w = ctx.font.measure_text(&self.label, style.font_size) + style.pad_h * 2.0;
            let h = style.font_size * 1.7;
            let label = self.label.clone();
            // TREE-ATTACHED so the engine's `AboveCentered` handling maps
            // the anchor through `content_to_screen` — a tooltip on a
            // widget inside a GPU scroll layer is remapped to window space
            // and centred over its anchor (the legacy Absolute push path
            // skipped that remap and dropped the label off-screen).
            ctx.attach_overlay(
                OverlayEntry::new(
                    LayerPosition::AboveCentered(r),
                    TooltipLabel { label, w, h, style },
                )
                .input(InputBehavior::PassThrough)
                .focus(FocusBehavior::Inert),
            );
        }
    }
}

struct TooltipLabel {
    label: String,
    w: f32,
    h: f32,
    style: TooltipStyle,
}

impl Widget for TooltipLabel {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.w, height: self.h })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        if self.style.elevation > 0.0 {
            ctx.fill_shadow_rrect(r, self.style.radius, Color::rgba(0, 0, 0, 90), 8.0);
        }
        ctx.fill_rrect(r, self.style.radius, self.style.background);
        let ty = r.origin.y + (self.h - ctx.font.line_height(self.style.font_size)) / 2.0;
        ctx.draw_text_at(
            &self.label,
            Point { x: r.origin.x + self.style.pad_h, y: ty },
            self.style.text_color,
            self.style.font_size,
        );
    }
}

/// Ergonomic extension available on EVERY widget (D115/Phase 32): attach a
/// tooltip as a PROPERTY instead of wrapping — `Button::new("Save")
/// .tooltip("Saves changes")`. Desktop shows it on hover; mobile has no
/// hover, so it's naturally inert there (a long-press variant can hook the
/// same path later). Styling comes from the theme's [`TooltipStyle`].
pub trait WidgetExt: Widget + Sized + 'static {
    /// Show `label` while the pointer hovers this widget.
    fn tooltip(self, label: impl Into<String>) -> Tooltip {
        Tooltip::new(label, self)
    }
}

impl<W: Widget + Sized + 'static> WidgetExt for W {}
