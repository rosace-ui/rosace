use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Full-page layout: optional AppBar + optional NavRail sidebar + body + optional FAB.
///
/// This is the root widget for any screen in a ROSACE app — analogous to
/// Flutter's `Scaffold`, SwiftUI's `NavigationSplitView`, or Compose's `Scaffold`.
pub struct Scaffold {
    pub background: Color,
    pub app_bar: Option<BoxedWidget>,
    pub nav_rail: Option<BoxedWidget>,
    pub body: BoxedWidget,
    pub fab: Option<BoxedWidget>,
    pub bottom_bar: Option<BoxedWidget>,
    pub sidebar_right: Option<BoxedWidget>,
}

impl Scaffold {
    pub fn new(body: impl Widget + 'static) -> Self {
        Self {
            background: Color::rgba(0, 0, 0, 0), // sentinel: use theme.background
            app_bar: None,
            nav_rail: None,
            body: Box::new(body),
            fab: None,
            bottom_bar: None,
            sidebar_right: None,
        }
    }

    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn app_bar(mut self, w: impl Widget + 'static) -> Self { self.app_bar = Some(Box::new(w)); self }
    pub fn nav_rail(mut self, w: impl Widget + 'static) -> Self { self.nav_rail = Some(Box::new(w)); self }
    pub fn fab(mut self, w: impl Widget + 'static) -> Self { self.fab = Some(Box::new(w)); self }
    pub fn bottom_bar(mut self, w: impl Widget + 'static) -> Self { self.bottom_bar = Some(Box::new(w)); self }
    pub fn sidebar_right(mut self, w: impl Widget + 'static) -> Self { self.sidebar_right = Some(Box::new(w)); self }
}

impl Widget for Scaffold {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let full = ctx.rect;

        // Background — use explicit color or fall back to theme.background.
        // Painted over the FULL rect (extends behind the status bar/notch,
        // matching normal mobile-app behavior); only the content below is
        // inset by the safe area.
        let bg = if self.background.a == 0 {
            ctx.tc(ctx.theme.colors.background)
        } else {
            self.background
        };
        ctx.fill_rect(full, bg);

        // Keep interactive content (AppBar, body, bottom bar, FAB) clear of
        // platform-reserved regions — iOS status bar/Dynamic Island/home
        // indicator, Android status/nav bars. Zero on platforms without one
        // (desktop, web), so this is a no-op there.
        let sa = rosace_core::use_safe_area();
        let total = Rect {
            origin: Point { x: full.origin.x + sa.left, y: full.origin.y + sa.top },
            size: Size {
                width: (full.size.width - sa.left - sa.right).max(0.0),
                height: (full.size.height - sa.top - sa.bottom).max(0.0),
            },
        };

        // Measure the bars LOOSE, so each reports the height it actually
        // needs.
        //
        // These were `Constraints::tight(width, 44.0)` / `tight(.., 48.0)`,
        // which FORCE the height — the bar's own `layout` result was
        // discarded and the constant used instead. That silently defeated
        // two things: an explicit `AppBar::height(56.0)`, and `AppBar`'s
        // deliberate growth with the OS text-size setting (its `layout` has
        // a comment explaining that the theme height is a minimum, not a
        // ceiling, added after raised Dynamic Type clipped the title on
        // iOS). Inside a Scaffold — the normal case — none of that applied.
        //
        // The old constants stay as FLOORS, so an ordinary bar is unchanged.
        let measure_bar = |w: &super::BoxedWidget, floor: f32| {
            let c = Constraints::loose(total.size.width, total.size.height);
            w.layout(&ctx.layout_ctx(c)).height.max(floor)
        };
        let bar_h = self.app_bar.as_ref().map(|w| measure_bar(w, 44.0)).unwrap_or(0.0);
        let bottom_h = self.bottom_bar.as_ref().map(|w| measure_bar(w, 48.0)).unwrap_or(0.0);

        // Paint app bar
        if let Some(bar) = &self.app_bar {
            bar.paint(&mut ctx.child(Rect {
                origin: total.origin,
                size: Size { width: total.size.width, height: bar_h },
            }));
        }

        // Content area (below bar, above bottom bar)
        let content_y = total.origin.y + bar_h;
        // Clamped: a window shorter than its own chrome (a phone in
        // landscape with both bars, a tiny embedded view) otherwise yields a
        // NEGATIVE height that flows straight into every child rect below.
        // The safe-area rect above already uses `.max(0.0)` for exactly this
        // class of problem — this line was the one that missed it.
        let content_h = (total.size.height - bar_h - bottom_h).max(0.0);

        // Measure nav rail
        let rail_w = self.nav_rail.as_ref()
            .map(|w| w.layout(&ctx.layout_ctx(Constraints::loose(300.0, content_h))).width)
            .unwrap_or(0.0);

        // Paint nav rail
        if let Some(rail) = &self.nav_rail {
            rail.paint(&mut ctx.child(Rect {
                origin: Point { x: total.origin.x, y: content_y },
                size: Size { width: rail_w, height: content_h },
            }));
        }

        // Measure right sidebar
        let rsb_w = self.sidebar_right.as_ref()
            .map(|w| w.layout(&ctx.layout_ctx(Constraints::loose(400.0, content_h))).width)
            .unwrap_or(0.0);

        // Paint right sidebar
        if let Some(rsb) = &self.sidebar_right {
            rsb.paint(&mut ctx.child(Rect {
                origin: Point { x: total.origin.x + total.size.width - rsb_w, y: content_y },
                size: Size { width: rsb_w, height: content_h },
            }));
        }

        // Paint body
        let body_x = total.origin.x + rail_w;
        let body_w = total.size.width - rail_w - rsb_w;
        self.body.paint(&mut ctx.child(Rect {
            origin: Point { x: body_x, y: content_y },
            size: Size { width: body_w, height: content_h },
        }));

        // Paint bottom bar
        if let Some(bb) = &self.bottom_bar {
            super::set_bottom_overlay_inset(bottom_h);
            bb.paint(&mut ctx.child(Rect {
                origin: Point { x: total.origin.x, y: total.origin.y + total.size.height - bottom_h },
                size: Size { width: total.size.width, height: bottom_h },
            }));
        }

        // FAB (bottom-right)
        if let Some(fab) = &self.fab {
            let fab_size = fab.layout(&ctx.layout_ctx(Constraints::loose(60.0, 60.0)));
            let fab_x = total.origin.x + total.size.width - fab_size.width - 20.0;
            let fab_y = total.origin.y + total.size.height - bottom_h - fab_size.height - 20.0;
            fab.paint(&mut ctx.child(Rect {
                origin: Point { x: fab_x, y: fab_y },
                size: fab_size,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::{FontCache, PictureRecorder};
    use crate::tree::RenderTree;
    use std::{cell::RefCell, rc::Rc};

    /// Paint a scaffold into `h` px tall and return every child rect the
    /// render tree recorded.
    fn child_rects(build: impl FnOnce() -> Scaffold, w: f32, h: f32) -> Vec<Rect> {
        let font = FontCache::embedded();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: w, height: h } },
                &font,
                rosace_theme::built_in::dark_theme(),
                tree.clone(),
            );
            build().paint(&mut ctx);
        }
        // Walk from the root; RenderTree exposes children, not a length.
        let t = tree.borrow();
        fn walk(t: &RenderTree, id: usize, out: &mut Vec<Rect>) {
            if let Some(r) = t.node(id).cached_rect { out.push(r); }
            for &c in &t.node(id).children { walk(t, c, out); }
        }
        let mut out = Vec::new();
        walk(&t, RenderTree::ROOT, &mut out);
        out
    }

    /// A window shorter than its own chrome must not produce negative rects.
    ///
    /// `content_h = height - bar_h - bottom_h` was unclamped, while the
    /// safe-area rect 25 lines above it already used `.max(0.0)` for exactly
    /// this. A phone in landscape with both bars, or a small embedded view,
    /// pushed a negative height into every child rect below.
    #[test]
    fn a_window_shorter_than_its_own_chrome_never_yields_a_negative_rect() {
        let rects = child_rects(
            || Scaffold::new(super::super::Container::new())
                .app_bar(super::super::AppBar::new("Title"))
                .bottom_bar(super::super::Container::new().height(48.0)),
            320.0,
            40.0, // shorter than the app bar alone
        );
        assert!(!rects.is_empty(), "the scaffold must still paint something");
        for r in &rects {
            assert!(r.size.width >= 0.0 && r.size.height >= 0.0,
                "negative rect {r:?} in a 40px-tall window");
        }
    }

    /// A bar whose height comes from its CONTENT gets the real space to
    /// measure in.
    ///
    /// The bars used to be measured with `Constraints::tight(width, 44.0)`.
    /// That does NOT force `AppBar` or a fixed-height `Container` — both
    /// return their own height and ignore the incoming one — so the tight
    /// constraint was inert for the common cases. What it did do was cap
    /// `avail_h` at the assumed height, so a content-sized bar measured its
    /// children against 44px of room instead of the window.
    #[test]
    fn a_content_sized_bar_measures_against_the_real_space() {
        let tall_child = 120.0_f32;
        let rects = child_rects(
            || Scaffold::new(super::super::Container::new())
                // No explicit height: this bar is as tall as its child.
                .app_bar(super::super::Container::new()
                    .child(super::super::Container::new().height(tall_child))),
            320.0,
            600.0,
        );
        let bar = rects.iter()
            .find(|r| r.origin.y == 0.0 && (r.size.width - 320.0).abs() < 0.5)
            .expect("the app bar rect should be full-width at the top");
        assert_eq!(
            bar.size.height, tall_child,
            "a content-sized bar was measured against the assumed height, not the window",
        );
    }

    /// ...and the designed height still acts as a FLOOR, so an ordinary bar
    /// is unchanged by the switch to loose constraints.
    #[test]
    fn a_short_bar_is_still_floored_at_the_designed_height() {
        let rects = child_rects(
            || Scaffold::new(super::super::Container::new())
                .app_bar(super::super::Container::new().height(10.0)),
            320.0,
            600.0,
        );
        assert!(
            rects.iter().all(|r| r.size.height >= 10.0),
            "a 10px bar should be floored, not shrunk further: {rects:?}",
        );
    }
}
