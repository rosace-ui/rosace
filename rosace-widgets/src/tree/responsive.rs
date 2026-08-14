//! `Responsive` — build a different tree depending on how much room there is.
//!
//! Until this existed, a layout could not react to its own size at all. The
//! only way to read a real width was [`super::RectReader`], which reports
//! `ctx.rect` AFTER paint into an `Atom` — so a responsive layout built that
//! way is always one frame behind, and flashes the wrong layout on the first
//! frame of every resize. That is fine for anchoring a popup (what
//! `RectReader` was built for) and wrong for choosing a layout.
//!
//! This is Flutter's `LayoutBuilder`: the closure runs during layout and
//! paint with the space actually available, so the correct tree is built the
//! first time.
//!
//! ```rust,ignore
//! Responsive::new(|space| {
//!     if space.width >= 700.0 {
//!         Box::new(Row::new().child(sidebar()).child(content()))
//!     } else {
//!         Box::new(Column::new().child(content()))
//!     }
//! })
//! ```
//!
//! The closure runs on every layout and paint pass, so keep it to assembling
//! widgets — it is not a place for expensive work.

use std::sync::Arc;

use rosace_core::types::Size;
use rosace_layout::Constraints;

use super::{BoxedWidget, LayoutCtx, PaintCtx, Widget};

/// Common width breakpoints, so apps do not each invent their own.
///
/// Deliberately only two. Material publishes five and iOS publishes two;
/// three named cases cover what a layout actually branches on, and every
/// extra breakpoint is one more combination to test.
pub mod breakpoint {
    /// Below this is a phone in portrait — one column, nothing beside it.
    pub const COMPACT: f32 = 600.0;
    /// At or above this there is room for a persistent side region (a nav
    /// rail, a detail pane).
    pub const EXPANDED: f32 = 900.0;
}

/// Builds its child from the space available to it. See the module docs.
pub struct Responsive {
    builder: Arc<dyn Fn(Size) -> BoxedWidget + Send + Sync>,
}

impl Responsive {
    pub fn new(builder: impl Fn(Size) -> BoxedWidget + Send + Sync + 'static) -> Self {
        Self { builder: Arc::new(builder) }
    }

    /// The space to hand the closure.
    ///
    /// An unbounded or shrink-to-fit axis is reported as 0, never as
    /// `f32::INFINITY`. A `ScrollView` measures its content against an
    /// unbounded axis, so a caller writing `if space.width >= 600.0` would
    /// otherwise silently take the widest branch inside every scroll view —
    /// the exact opposite of what a narrow-screen check means.
    fn space(c: Constraints) -> Size {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        Size { width: finite(c.max_width_f32()), height: finite(c.max_height_f32()) }
    }
}

impl Widget for Responsive {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        (self.builder)(Self::space(ctx.constraints)).layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Build against the rect actually allotted, which is what the parent
        // decided AFTER seeing our `layout` — not the incoming constraints.
        let child = (self.builder)(ctx.rect.size);
        let r = ctx.rect;
        ctx.paint_child(r, &*child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::FontCache;

    fn size_at(width: f32) -> Size {
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = Constraints::loose(width, 400.0);
        let w = Responsive::new(|space| {
            if space.width >= breakpoint::COMPACT {
                Box::new(super::super::Spacer::new(111.0))
            } else {
                Box::new(super::super::Spacer::new(22.0))
            }
        });
        w.layout(&LayoutCtx::new(c, &font, &theme))
    }

    #[test]
    fn the_branch_is_chosen_from_the_available_width_during_layout() {
        // Not one frame later, which is what a RectReader-based version does.
        assert_eq!(size_at(500.0).height, 22.0, "narrow branch");
        assert_eq!(size_at(800.0).height, 111.0, "wide branch");
    }

    #[test]
    fn an_unbounded_axis_reports_zero_not_infinity() {
        // Inside a ScrollView the cross axis is measured against infinity.
        // Reporting that verbatim would make every `>= 600.0` check true, so
        // a "phone" layout would silently take its widest branch.
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Size { width: -1.0, height: -1.0 }));
        let s = seen.clone();
        let w = Responsive::new(move |space| {
            *s.lock().unwrap() = space;
            Box::new(super::super::Spacer::new(1.0))
        });
        let c = Constraints {
            min_width: 0.0,
            max_width: rosace_core::AxisBound::Unbounded,
            min_height: 0.0,
            max_height: rosace_core::AxisBound::Bounded(300.0),
        };
        let _ = w.layout(&LayoutCtx::new(c, &font, &theme));

        let got = *seen.lock().unwrap();
        assert_eq!(got.width, 0.0, "unbounded width must be reported as 0");
        assert_eq!(got.height, 300.0, "the bounded axis is passed through");
    }
}
