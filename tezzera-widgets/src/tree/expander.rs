use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};

/// A collapsible section: a clickable header row (title + chevron) with a body
/// that shows only while `expanded` is true. Distinct behavior (toggle reveal)
/// from any existing widget.
pub struct Expander {
    title: String,
    expanded: Atom<bool>,
    body: BoxedWidget,
}

impl Expander {
    pub fn new(title: impl Into<String>, expanded: Atom<bool>, body: impl Widget + 'static) -> Self {
        Self { title: title.into(), expanded, body: Box::new(body) }
    }
}

const HEADER_H: f32 = 44.0;

impl Widget for Expander {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = super::avail_w(ctx.constraints);
        let mut h = HEADER_H;
        if self.expanded.get() {
            let bc = Constraints::loose(w, f32::INFINITY);
            h += self.body.layout(&ctx.with_constraints(bc)).height + 8.0;
        }
        ctx.constraints.constrain(Size { width: w, height: h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let fg = ctx.tc(ctx.theme.colors.on_surface);
        let open = self.expanded.get();

        // Header
        let header = Rect { origin: r.origin, size: Size { width: r.size.width, height: HEADER_H } };
        let lh = ctx.font.line_height(15.0);
        ctx.draw_text_at(&self.title, Point { x: r.origin.x, y: r.origin.y + (HEADER_H - lh) / 2.0 }, fg, 15.0);
        // Chevron (▸ collapsed / ▾ expanded)
        let chev = if open { "\u{25be}" } else { "\u{25b8}" };
        let cw = ctx.font.measure_text(chev, 15.0);
        ctx.draw_text_at(chev, Point { x: r.origin.x + r.size.width - cw - 4.0, y: r.origin.y + (HEADER_H - lh) / 2.0 }, fg, 15.0);

        let atom = self.expanded.clone();
        let header_ctx = ctx.child(header);
        header_ctx.semantics(
            super::Semantics::new(tezzera_core::Role::Button)
                .label(&self.title)
                .value(if open { "expanded" } else { "collapsed" }),
        );
        header_ctx.register_hit(std::sync::Arc::new(move || atom.set(!atom.get())));

        if open {
            let bc = Constraints::loose(r.size.width, f32::INFINITY);
            let bs = self.body.layout(&ctx.layout_ctx(bc));
            let body_rect = Rect {
                origin: Point { x: r.origin.x, y: r.origin.y + HEADER_H + 4.0 },
                size: Size { width: r.size.width, height: bs.height },
            };
            self.body.paint(&mut ctx.child(body_rect));
        }
    }
}
