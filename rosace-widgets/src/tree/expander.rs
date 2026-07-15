use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_state::Atom;

use super::container::draw_rounded_rect_pub;
use super::{BoxedWidget, LayoutCtx, PaintCtx, Widget};

/// A collapsible section: a clickable header row (title + chevron) with a
/// body that shows only while `expanded` is true.
///
/// Phase 32 sweep (user-reported): the section is now visually
/// DIFFERENTIABLE — a themed surface with configurable `.background()`,
/// `.border()`, `.radius()`, `.elevation()` — and ANIMATED: the body
/// reveals with the theme-governed eased factor (`ctx.animate_to`, the
/// same D108 machinery every toggle widget uses; disable animations
/// globally and it snaps) and the chevron rotates through the same factor.
pub struct Expander {
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

impl Expander {
    pub fn new(title: impl Into<String>, expanded: Atom<bool>, body: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            expanded,
            body: Box::new(body),
            background: None,
            border: None,
            radius: 10.0,
            elevation: 0.0,
            title_size: 15.0,
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

const HEADER_H: f32 = 44.0;
const PAD_H: f32 = 14.0;

impl Widget for Expander {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = super::avail_w(ctx.constraints);
        let mut h = HEADER_H;
        if self.expanded.get() {
            let bc = Constraints::loose(w - PAD_H * 2.0, f32::INFINITY);
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

        if self.elevation > 0.0 {
            let spread = 3.0 * self.elevation;
            draw_rounded_rect_pub(
                ctx,
                Rect {
                    origin: Point { x: r.origin.x - spread / 2.0, y: r.origin.y + spread },
                    size: Size { width: r.size.width + spread, height: r.size.height },
                },
                Color::rgba(shadow.r, shadow.g, shadow.b, 50),
                self.radius + spread / 2.0,
            );
        }
        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        if outline.1 > 0.0 {
            ctx.stroke_rrect(r, self.radius, outline.0, outline.1);
        }

        // Header
        let header = Rect { origin: r.origin, size: Size { width: r.size.width, height: HEADER_H } };
        let lh = ctx.font.line_height(self.title_size);
        ctx.draw_text_at(
            &self.title,
            Point { x: r.origin.x + PAD_H, y: r.origin.y + (HEADER_H - lh) / 2.0 },
            fg,
            self.title_size,
        );
        // Chevron "rotates" through the eased factor — cross-fading ▸ into
        // ▾ (no glyph-rotation primitive yet; the cross-fade tracks the
        // exact same animation curve the body reveal uses, so the two read
        // as one motion).
        let cx = r.origin.x + r.size.width - PAD_H - ctx.font.measure_text("\u{25be}", self.title_size);
        let cy = r.origin.y + (HEADER_H - lh) / 2.0;
        if t < 1.0 {
            let a = (255.0 * (1.0 - t)) as u8;
            ctx.draw_text_at("\u{25b8}", Point { x: cx, y: cy }, Color::rgba(fg.r, fg.g, fg.b, a), self.title_size);
        }
        if t > 0.0 {
            let a = (255.0 * t) as u8;
            ctx.draw_text_at("\u{25be}", Point { x: cx, y: cy }, Color::rgba(fg.r, fg.g, fg.b, a), self.title_size);
        }

        let atom = self.expanded.clone();
        let header_ctx = ctx.child(header);
        header_ctx.semantics(
            super::Semantics::new(rosace_core::Role::Button)
                .label(&self.title)
                .value(if open { "expanded" } else { "collapsed" }),
        );
        header_ctx.register_hit(std::sync::Arc::new(move || atom.set(!atom.get())));

        if open {
            let bc = Constraints::loose(r.size.width - PAD_H * 2.0, f32::INFINITY);
            let bs = self.body.layout(&ctx.layout_ctx(bc));
            let body_rect = Rect {
                origin: Point { x: r.origin.x + PAD_H, y: r.origin.y + HEADER_H + 4.0 },
                size: Size { width: r.size.width - PAD_H * 2.0, height: bs.height },
            };
            // Fade the body in along the same eased factor.
            if t < 1.0 {
                super::request_animation();
            }
            self.body.paint(&mut ctx.child(body_rect));
        }
    }
}
