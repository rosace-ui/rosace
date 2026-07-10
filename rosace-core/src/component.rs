use crate::context::Context;
use crate::element::Element;
use crate::types::ComponentId;

/// The core trait every ROSACE component implements.
///
/// A component is a pure function from props (`&self`) and [`Context`] to an
/// [`Element`] tree. The framework calls `build` every frame (Phase 1) or when
/// the component is marked dirty by a state change (Phase 2+).
///
/// # Example
/// ```rust,ignore
/// struct Greeting { name: String }
///
/// impl Component for Greeting {
///     fn build(&self, ctx: &mut Context) -> Element {
///         Text::new(format!("Hello, {}!", self.name)).into_element()
///     }
/// }
/// ```
pub trait Component: Send + Sync + 'static {
    /// Produce the element tree for this component.
    fn build(&self, ctx: &mut Context) -> Element;

    /// Called once after the component first appears in the tree.
    ///
    /// Default implementation is a no-op. Override for side effects that
    /// should run on mount (e.g., starting timers, subscriptions).
    fn on_mount(&self) {}

    /// Called once after the component is removed from the tree.
    ///
    /// Default implementation is a no-op. Override for cleanup that cannot
    /// be expressed as a `ctx.on_cleanup` closure (e.g., releasing platform
    /// resources held by `self`).
    fn on_unmount(&self) {}

    /// Fully-qualified type name used in diagnostics.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Convert this component into an [`Element`] so it can be embedded
    /// inside another component's `build()` output.
    ///
    /// The reconciler assigns the real position-based [`ComponentId`] during
    /// the tree walk; `ComponentId(0)` here is a placeholder.
    fn into_element(self) -> Element
    where
        Self: Sized,
    {
        use crate::element::{ComponentElement, Element};
        use std::sync::Arc;

        Element::Component(ComponentElement {
            id: ComponentId(0),
            key: None,
            component: Arc::new(self),
            children: vec![],
        })
    }
}
