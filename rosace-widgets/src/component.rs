//! `Component` — what an app author implements to build a screen.
//!
//! It lives HERE rather than in `rosace-core` for one reason: `build` returns a
//! widget, and `Widget` is defined in this crate. Moving the trait down to the
//! widgets layer keeps `rosace-core` the light foundation it is (it depends only
//! on `rosace-trace` and `rosace-state`); moving `Widget` UP into core would
//! have dragged `rosace-render`, `rosace-theme` and `rosace-layout` with it and
//! inverted the workspace's layering.
//!
//! `Context` stays in `rosace-core` — it only needs `ComponentId`, and the hook
//! machinery (`ctx.state`, `ctx.on_cleanup`) is unchanged.

use rosace_core::Context;

use crate::tree::BoxedWidget;

/// The trait every ROSACE screen implements.
///
/// ```rust,ignore
/// struct Greeting { name: String }
///
/// impl Component for Greeting {
///     fn build(&self, _ctx: &mut Context) -> BoxedWidget {
///         Text::new(format!("Hello, {}!", self.name)).boxed()
///     }
/// }
/// ```
///
/// A component is a pure function from props (`&self`) and [`Context`] to a
/// widget tree. The framework calls `build` when the component is marked dirty
/// by a state change, and reuses the last result otherwise.
pub trait Component: Send + Sync + 'static {
    /// Produce this component's widget tree.
    fn build(&self, ctx: &mut Context) -> BoxedWidget;

    /// Ran once after the component first appears in the tree.
    fn on_mount(&self) {}

    /// Ran once after the component is removed from the tree. For cleanup that
    /// cannot be expressed as a `ctx.on_cleanup` closure — releasing a platform
    /// resource held by `self`, say.
    fn on_unmount(&self) {}

    /// Fully-qualified type name, used in diagnostics.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// `w.boxed()` instead of `Arc::new(w)` at the end of every `build`.
pub trait IntoBoxedWidget: crate::tree::Widget + Sized + 'static {
    fn boxed(self) -> BoxedWidget {
        std::sync::Arc::new(self)
    }
}

impl<W: crate::tree::Widget + Sized + 'static> IntoBoxedWidget for W {}
