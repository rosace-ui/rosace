use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Full-page layout: optional AppBar + optional NavRail sidebar + body + optional FAB.
///
/// This is the root widget for any screen in a TEZZERA app — analogous to
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
        let sa = tezzera_core::use_safe_area();
        let total = Rect {
            origin: Point { x: full.origin.x + sa.left, y: full.origin.y + sa.top },
            size: Size {
                width: (full.size.width - sa.left - sa.right).max(0.0),
                height: (full.size.height - sa.top - sa.bottom).max(0.0),
            },
        };

        // Measure app bar
        let bar_h = self.app_bar.as_ref()
            .map(|w| w.layout(&ctx.layout_ctx(Constraints::tight(total.size.width, 44.0))).height)
            .unwrap_or(0.0);

        // Measure bottom bar
        let bottom_h = self.bottom_bar.as_ref()
            .map(|w| w.layout(&ctx.layout_ctx(Constraints::tight(total.size.width, 48.0))).height)
            .unwrap_or(0.0);

        // Paint app bar
        if let Some(bar) = &self.app_bar {
            bar.paint(&mut ctx.child(Rect {
                origin: total.origin,
                size: Size { width: total.size.width, height: bar_h },
            }));
        }

        // Content area (below bar, above bottom bar)
        let content_y = total.origin.y + bar_h;
        let content_h = total.size.height - bar_h - bottom_h;

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
