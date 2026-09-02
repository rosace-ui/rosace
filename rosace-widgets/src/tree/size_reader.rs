use std::sync::Arc;

use rosace_core::types::{Rect, Size};

use super::{BoxedWidget, Children, PaintCtx, Widget};

/// Reports a widget's size, and only when it CHANGES.
///
/// ```rust,ignore
/// Chart::new(data).on_size(move |s| controller.set_viewport(s))
/// ```
///
/// Available on every widget through [`SizeApi`]. A channel OUT, like
/// [`super::RectReader`]: the size is produced by layout and read by the
/// caller, so it is a callback rather than a value the caller owns.
///
/// ## Why change-gating is the whole point
///
/// The obvious version fires on every paint. That is not a smaller version of
/// this — it is a different, worse thing:
///
///   * a handler that writes app state would mark something dirty every
///     frame, and the app never returns to an idle frame. The per-node
///     caching this engine is built on stops paying for itself the moment one
///     widget does that;
///   * the caller has to remember the last value and compare it anyway, so
///     every caller reimplements this, and any caller that forgets has a
///     rebuild loop that is invisible until profiled.
///
/// The comparison lives on the NODE (`widget_state`), so it survives across
/// frames and is per-identity: two of these in a list do not share a memory.
/// It is written with `set_quietly` — recording what was already observed
/// must not schedule another frame, or the gate would defeat itself.
pub struct SizeReader {
    on_size: Arc<dyn Fn(Size) + Send + Sync>,
    child: BoxedWidget,
}

impl SizeReader {
    pub fn new(child: impl Widget + 'static, on_size: impl Fn(Size) + Send + Sync + 'static) -> Self {
        Self { on_size: Arc::new(on_size), child: Arc::new(child) }
    }
}

impl Widget for SizeReader {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        // Unconditional and first, so the slot index is stable — see
        // `widget_state`'s own assertion.
        let last = ctx.widget_state(|| None::<Size>);

        ctx.paint_child(rect, &*self.child);

        if last.with(|prev| *prev != Some(rect.size)) {
            last.set_quietly(Some(rect.size));
            (self.on_size)(rect.size);
        }
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}

/// Same, for the full rect — position as well as size.
///
/// Position matters for anchoring something to a widget; size alone is the
/// common case and gets the shorter name.
pub struct BoundsReader {
    on_bounds: Arc<dyn Fn(Rect) + Send + Sync>,
    child: BoxedWidget,
}

impl BoundsReader {
    pub fn new(child: impl Widget + 'static, on_bounds: impl Fn(Rect) + Send + Sync + 'static) -> Self {
        Self { on_bounds: Arc::new(on_bounds), child: Arc::new(child) }
    }
}

impl Widget for BoundsReader {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        let last = ctx.widget_state(|| None::<Rect>);

        ctx.paint_child(rect, &*self.child);

        if last.with(|prev| *prev != Some(rect)) {
            last.set_quietly(Some(rect));
            (self.on_bounds)(rect);
        }
    }
}

/// Builder sugar on every widget: `.on_size(..)` and `.on_bounds(..)`.
pub trait SizeApi: Widget + Sized + Send + Sync + 'static {
    /// Called when this widget's SIZE changes, and on its first paint.
    fn on_size(self, f: impl Fn(Size) + Send + Sync + 'static) -> SizeReader {
        SizeReader::new(self, f)
    }

    /// Called when this widget's rect changes — position or size — and on its
    /// first paint. Use when anchoring something to where it sits;
    /// [`Self::on_size`] is the common case.
    fn on_bounds(self, f: impl Fn(Rect) + Send + Sync + 'static) -> BoundsReader {
        BoundsReader::new(self, f)
    }
}

impl<W: Widget + Send + Sync + 'static> SizeApi for W {}
