//! Companion demo for `.steering/WIDGET_AUTHORING_GUIDE.md` — one worked
//! example per taxonomy row (leaf / single-child wrapper / multi-child
//! container), verified against the REAL `Widget` trait as it ships today
//! (not the aspirational `WIDGET_PROTOCOL.md` planning API — no
//! `layout_child`/`position_child`/`SemanticsCx`, which were scoped down or
//! deferred during Phase 21). Exists so the guide's code samples are proven
//! to compile, not just prose.

use tezzera::prelude::*;
// `Children` and `LayoutCtx` aren't in the prelude (most app code never
// implements Widget directly) — reach into tezzera::widgets::tree for them.
use tezzera::widgets::tree::{Children, LayoutCtx};

// ── 1. Leaf: draws content, has no children (Children::None default) ───────
//
// Implement `layout` + `paint`; everything else is defaulted.

struct Dot {
    radius: f32,
    color: Color,
}

impl Widget for Dot {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let d = self.radius * 2.0;
        ctx.constraints.constrain(Size { width: d, height: d })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let center = Point {
            x: ctx.rect.origin.x + ctx.rect.size.width / 2.0,
            y: ctx.rect.origin.y + ctx.rect.size.height / 2.0,
        };
        ctx.fill_circle(center, self.radius, self.color);
    }
}

// ── 2. Single-child wrapper: decorates one child ────────────────────────────
//
// `children() -> Children::One(&self.child)` gives you free `flex_factor`
// delegation (a Highlight inside a Row/Column's flex slot behaves like its
// child). Layout is left at its default (= child's size). Paint is
// overridden: draw the glow, then paint the child in the same rect.

struct Highlight {
    child: Box<dyn Widget>,
    glow: Color,
}

impl Widget for Highlight {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_shadow_rrect(ctx.rect, 12.0, self.glow, 16.0);
        let rect = ctx.rect;
        self.child.paint(&mut ctx.child(rect));
    }
    // layout: defaulted to the child's size via Children::One.
}

// ── 3. Multi-child container: arranges several children ─────────────────────
//
// `Children::Many` has no default layout (there's no universal multi-child
// arrangement) — layout must be overridden. Paint is also overridden here
// because children are positioned at explicit offsets, not all stacked in
// the same rect (the `Many` default).

struct EvenColumn {
    items: Vec<Box<dyn Widget>>,
    gap: f32,
}

impl Widget for EvenColumn {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let (mut y, mut w) = (0.0f32, 0.0f32);
        for item in &self.items {
            let s = item.layout(ctx);
            y += s.height + self.gap;
            w = w.max(s.width);
        }
        ctx.constraints.constrain(Size {
            width: w,
            height: (y - self.gap).max(0.0),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let mut y = ctx.rect.origin.y;
        for item in &self.items {
            let s = item.layout(&ctx.layout_ctx(Constraints::loose(ctx.rect.size.width, f32::INFINITY)));
            let child_rect = Rect {
                origin: Point { x: ctx.rect.origin.x, y },
                size: Size { width: ctx.rect.size.width, height: s.height },
            };
            item.paint(&mut ctx.child(child_rect));
            y += s.height + self.gap;
        }
    }
}

// ── Demo app ─────────────────────────────────────────────────────────────────

struct AuthoringDemo;

impl Component for AuthoringDemo {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .spacing(20.0)
                .child(Text::new("Leaf — Dot"))
                .child(Dot { radius: 24.0, color: Color::rgb(235, 110, 75) })
                .child(Text::new("Single-child wrapper — Highlight"))
                .child(Highlight {
                    child: Box::new(Text::new("glowing text")),
                    glow: Color::rgb(255, 200, 60),
                })
                .child(Text::new("Multi-child container — EvenColumn"))
                .child(EvenColumn {
                    items: vec![
                        Box::new(Text::new("row one")),
                        Box::new(Text::new("row two")),
                        Box::new(Text::new("row three")),
                    ],
                    gap: 8.0,
                }),
        )
        .app_bar(AppBar::new("Widget Authoring Examples"))
        .into_element()
    }
}

fn main() {
    App::new()
        .title("Widget Authoring Examples")
        .size(480, 520)
        .launch(AuthoringDemo);
}
