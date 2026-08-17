//! `StatefulWidget` — a widget that rebuilds its own children.
//!
//! # What makes it different from every other widget
//!
//! Under a TARGETED frame nothing rebuilds. A `Column` constructed with two
//! children still holds exactly those two `BoxedWidget`s, because nothing
//! re-ran to produce different ones. So an ordinary widget can change how it
//! LOOKS from node state — `Button` does, through `ctx.pressed()` and
//! `ctx.animate_to()` — but it cannot change WHAT IT CONTAINS.
//!
//! A `StatefulWidget` owns a `build` that re-runs when the widget is marked
//! dirty. That is the whole distinction, and the only reason this type exists.
//!
//! # State is yours, not ours
//!
//! The framework stores no state. Keep it in your own struct, a store, a BLoC
//! — wherever suits the app's architecture. All the framework does is note
//! that this widget is stale and run `build` again on the next frame; the walk
//! then does the necessary work, with the per-node caches deciding what can be
//! replayed.
//!
//! ```ignore
//! struct Panel { count: AtomicU32 }
//!
//! impl StatefulWidget for Panel {
//!     fn build(&self) -> BoxedWidget {
//!         let n = self.count.load(Ordering::Relaxed);
//!         Column::new()
//!             .child(Text::new(format!("{n}")))
//!             .child(Button::new("+").on_press(|| {
//!                 // no handle threaded through build
//!                 refresh_state();
//!             }))
//!             .boxed()
//!     }
//!
//!     fn on_dispose(&self) { /* cancel subscriptions */ }
//! }
//! ```
//!
//! # Lifecycle is opt-in
//!
//! `on_mount` and `on_dispose` are always available — tree membership is
//! something the framework observes for free. `on_lifecycle` (app foreground /
//! background) is only delivered to widgets that override it, because on a
//! given page many widgets manage state and typically one cares about the app
//! moving to the background. Jetpack Compose draws the same line: `remember`
//! is everyday and universal, `LocalLifecycleOwner` is a separate, rare opt-in.

use std::sync::Arc;

use rosace_core::types::Size;

use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// A widget that rebuilds its own subtree when refreshed.
///
/// Implement this on your own type, then place it with [`Stateful::new`] (or
/// [`StatefulExt::stateful`]). It is a separate trait rather than a blanket
/// `impl Widget` because a blanket impl would overlap with every concrete
/// widget's own impl and collide with coherence — the same reason D098 chose
/// one `Widget` trait with `children()` over Flutter's subclass families.
pub trait StatefulWidget: Send + Sync + 'static {
    /// Produce this widget's subtree. Re-runs whenever the widget is marked
    /// dirty by [`refresh_state`](super::refresh_state) or
    /// [`refresh`](super::refresh).
    fn build(&self) -> BoxedWidget;

    /// Ran once, when this widget first enters the tree.
    fn on_mount(&self) {}

    /// Ran when this widget leaves the tree — its slot was dropped, or taken
    /// over by a widget of a different type. Release subscriptions, timers and
    /// handles here.
    ///
    /// Children are disposed before their parents.
    fn on_dispose(&self) {}

    /// Opt-in: the app moved between foreground and background.
    ///
    /// Only widgets that override this are considered registered. See
    /// [`rosace_core::app_lifecycle::LifecycleState`] for what each phase
    /// means and which are safe to do work in.
    fn on_lifecycle(&self, _phase: rosace_core::app_lifecycle::LifecycleState) {}

    /// Whether this widget wants [`Self::on_lifecycle`]. Override alongside it.
    ///
    /// Explicit rather than inferred because Rust cannot tell a default method
    /// body from an override at runtime, and delivering the callback to every
    /// widget on every phase change would defeat the point of it being rare.
    fn observes_lifecycle(&self) -> bool { false }
}

/// Places a [`StatefulWidget`] into the tree.
///
/// Holds nothing but the widget itself. The built subtree and the mounted flag
/// live on the NODE, because this object does not survive: a structural frame
/// re-runs the enclosing `build()` and constructs a fresh `Stateful`. Keeping
/// them here meant every rebuild looked like a first paint — `on_mount` fired
/// again and another `on_dispose` closure was pushed onto the node, so N
/// rebuilds produced N mounts and N disposals.
///
/// That is the same reason Flutter puts `State` on the Element rather than the
/// Widget, and SwiftUI puts `@State`'s storage in the attribute graph rather
/// than in the `View` struct. The description is disposable; the node is not.
pub struct Stateful<T: StatefulWidget> {
    inner: Arc<T>,
}

impl<T: StatefulWidget> Stateful<T> {
    pub fn new(inner: T) -> Self {
        Self { inner: Arc::new(inner) }
    }
}

impl<T: StatefulWidget> Widget for Stateful<T> {
    fn children(&self) -> Children<'_> {
        // The built subtree lives on the node behind a `RefCell`, so it cannot
        // be lent out with the lifetime `Children` requires. Reporting `None`
        // keeps the protocol defaults away; `layout` and `paint` are both
        // overridden below, which is everything the defaults would have driven.
        Children::None
    }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let inner = Arc::clone(&self.inner);
        let built = ctx.built_child(move || inner.build());
        // Measured INLINE, deliberately — not through `ctx.layout_child`.
        //
        // A rebuild hands back a brand-new subtree, and a node cannot tell a
        // fresh-but-identical widget from a changed one; its `last_constraints`
        // still match and its `needs_layout` is still false, so `layout_child`
        // returns the size the OLD subtree had. That is the same hazard
        // `paint_child` avoids by refusing its cache on a structural frame,
        // except a `refresh_state()` frame is TARGETED, so nothing switches the
        // cache off.
        //
        // Caching here would need the rebuild to invalidate the slot the
        // subtree occupies, which means reaching into `children[0]` on an
        // assumption about how it was allocated. Not worth it: this is one
        // child, and correctness is not negotiable. Caught by
        // `size_propagation.rs` — a grandchild that grew stayed at its old
        // height.
        built.layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Mount is keyed to the NODE, so it happens once per stay in the tree
        // rather than once per rebuild. `adopt_tag` and `dispose_node_contents`
        // both clear the flag, so leaving and returning is correctly a new
        // mount and both removal paths reset it for free.
        if ctx.mark_mounted() {
            self.inner.on_mount();
            let inner = Arc::clone(&self.inner);
            ctx.on_dispose(move || inner.on_dispose());
            if self.inner.observes_lifecycle() {
                super::register_lifecycle_observer(ctx.node_id(), Arc::clone(&self.inner) as _);
            }
        }

        // `refresh_state()` inside this subtree must resolve to THIS widget.
        let _scope = super::enter_widget(ctx.node_id());
        let inner = Arc::clone(&self.inner);
        let built = ctx.built_child(move || inner.build());
        let rect = ctx.rect;
        ctx.paint_child(rect, &*built);
    }
}

/// `MyWidget.stateful()` instead of `Stateful::new(MyWidget)`.
pub trait StatefulExt: StatefulWidget + Sized {
    fn stateful(self) -> Stateful<Self> {
        Stateful::new(self)
    }
}

impl<T: StatefulWidget + Sized> StatefulExt for T {}
