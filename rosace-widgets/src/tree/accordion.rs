use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_state::Atom;

use super::container::draw_rounded_rect_pub;
use super::{BoxedWidget, EdgeInsets, LayoutCtx, PaintCtx, Widget};

/// A collapsible section: a clickable header row (title + chevron) with a
/// body that shows only while `expanded` is true.
///
/// Phase 32 sweep (user-reported): the section is now visually
/// DIFFERENTIABLE — a themed surface with configurable `.background()`,
/// `.border()`, `.radius()`, `.elevation()` — and ANIMATED: the body
/// reveals with the theme-governed eased factor (`ctx.animate_to`, the
/// same D108 machinery every toggle widget uses; disable animations
/// globally and it snaps) and the chevron rotates through the same factor.
pub struct Accordion {
    /// `None` = `EdgeInsets::symmetric(PAD_H, 0.0)`.
    padding: Option<EdgeInsets>,
    title: String,
    expanded: Atom<bool>,
    body: BoxedWidget,
    background: Option<Color>,
    border: Option<(Color, f32)>,
    radius: f32,
    /// Shadow strength; `0.0` disables (same convention as FAB).
    elevation: f32,
    title_size: f32,
}

impl Accordion {
    pub fn new(title: impl Into<String>, expanded: Atom<bool>, body: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            padding: None,
            expanded,
            body: Box::new(body),
            background: None,
            border: None,
            radius: 10.0,
            elevation: 0.0,
            title_size: 17.0,
        }
    }
    /// Panel fill — defaults to the theme's `surface`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Outline — defaults to a hairline of the theme's `outline`.
    pub fn border(mut self, c: Color, width: f32) -> Self { self.border = Some((c, width)); self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn elevation(mut self, e: f32) -> Self { self.elevation = e; self }
    pub fn title_size(mut self, s: f32) -> Self { self.title_size = s; self }
}

/// Designed header height — a MINIMUM. `header_height` grows it to fit
/// scaled text; taken literally it clipped the title at large OS text sizes.
const HEADER_H: f32 = 44.0;
/// Default horizontal inset. Overridable via `Accordion::padding`.
const PAD_H: f32 = 14.0;

impl Accordion {
    /// Inset around the header text and the expanded body.
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = Some(p); self }

    /// Header height: the designed 44 unless scaled text needs more.
    fn header_height(&self, font: &rosace_render::FontCache, font_size: f32) -> f32 {
        super::text_fit_height(HEADER_H, font, font_size)
    }

    fn insets(&self) -> EdgeInsets {
        self.padding.unwrap_or(EdgeInsets::symmetric(PAD_H, 0.0))
    }
}

impl Widget for Accordion {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = super::avail_w(ctx.constraints);
        let mut h = self.header_height(ctx.font, self.title_size);
        if self.expanded.get() {
            let bc = Constraints::loose(w - self.insets().total_h(), f32::INFINITY);
            h += self.body.layout(&ctx.with_constraints(bc)).height + 12.0;
        }
        ctx.constraints.constrain(Size { width: w, height: h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (borrow must end before mutable painting).
        let (bg, fg, outline, shadow) = {
            let t = &ctx.theme.colors;
            (
                self.background.unwrap_or_else(|| ctx.tc(t.surface)),
                ctx.tc(t.on_surface),
                self.border.unwrap_or((ctx.tc(t.outline), 1.0)),
                ctx.tc(t.shadow),
            )
        };
        let r = ctx.rect;
        let open = self.expanded.get();
        // Theme-eased reveal factor (0 collapsed → 1 expanded); drives the
        // body fade and the chevron rotation together.
        let t = ctx.animate_to(if open { 1.0 } else { 0.0 }, 0.0);

        // Real blurred drop shadow (`ctx.fill_shadow_rrect`, same primitive
        // Card/Container/FAB use) — this used to be a single flat,
        // hard-edged rounded rect offset below the panel, which reads as a
        // second solid gray box stacked behind it rather than a soft
        // shadow (2026-08-01 user feedback: "looks like another grey box").
        if self.elevation > 0.0 {
            ctx.fill_shadow_rrect(r, self.radius, Color::rgba(shadow.r, shadow.g, shadow.b, 90), 3.0 * self.elevation);
        }
        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        if outline.1 > 0.0 {
            ctx.stroke_rrect(r, self.radius, outline.0, outline.1);
        }

        // Header
        // Resolved ONCE and reused everywhere below: layout and paint
        // disagreeing on the header height puts the chevron and the body at
        // different offsets than the hit region.
        let header_h = self.header_height(ctx.font, self.title_size);
        let header = Rect { origin: r.origin, size: Size { width: r.size.width, height: header_h } };
        let lh = ctx.font.line_height(self.title_size);
        ctx.draw_text_at(
            &self.title,
            Point { x: r.origin.x + self.insets().left, y: r.origin.y + (header_h - lh) / 2.0 },
            fg,
            self.title_size,
        );
        // Chevron "rotates" through the eased factor — cross-fading
        // ChevronRight into ChevronDown (no glyph-rotation primitive yet;
        // the cross-fade tracks the exact same animation curve the body
        // reveal uses, so the two read as one motion). Real Icons (bundled
        // Material Symbols font, baked into the binary) instead of raw
        // Unicode ▸/▾ drawn through the body-text font — that font (Inter)
        // has no glyph for them, which rendered as a garbled/tofu box on
        // Android (no OS-level font-fallback there, unlike desktop).
        let chev_size = self.title_size + 2.0;
        let cx = r.origin.x + r.size.width - self.insets().right - chev_size;
        let cy = r.origin.y + (header_h - chev_size) / 2.0;
        let chev_rect = Rect { origin: Point { x: cx, y: cy }, size: Size { width: chev_size, height: chev_size } };
        if t < 1.0 {
            let a = (255.0 * (1.0 - t)) as u8;
            ctx.paint_child(chev_rect, &super::Icon::new(super::IconKind::ChevronRight)
                .size(chev_size)
                .color(Color::rgba(fg.r, fg.g, fg.b, a)));
        }
        if t > 0.0 {
            let a = (255.0 * t) as u8;
            ctx.paint_child(chev_rect, &super::Icon::new(super::IconKind::ChevronDown)
                .size(chev_size)
                .color(Color::rgba(fg.r, fg.g, fg.b, a)));
        }

        let atom = self.expanded.clone();
        let header_ctx = ctx.child(header);
        header_ctx.semantics(
            super::SemanticsProps::new(rosace_core::Role::Button)
                .label(&self.title)
                .value(if open { "expanded" } else { "collapsed" }),
        );
        header_ctx.register_hit(std::sync::Arc::new(move || atom.set(!atom.get())));

        if open {
            let pad = self.insets();
            let bc = Constraints::loose(r.size.width - pad.total_h(), f32::INFINITY);
            let bs = self.body.layout(&ctx.layout_ctx(bc));
            let body_rect = Rect {
                origin: Point { x: r.origin.x + pad.left, y: r.origin.y + header_h + 4.0 },
                size: Size { width: r.size.width - pad.total_h(), height: bs.height },
            };
            // Fade the body in along the same eased factor.
            if t < 1.0 {
                super::request_animation();
            }
            ctx.paint_child(body_rect, &*self.body);
        }
    }
}
