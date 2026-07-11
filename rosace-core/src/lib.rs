pub mod app;
pub mod child_container;
pub mod component;
pub mod context;
pub mod element;
pub mod error;
pub mod error_boundary;
pub mod lifecycle;
pub mod platform;
pub mod render_object;
pub mod safe_area;
pub mod semantic_node;
pub mod shader;
pub mod types;

pub use app::App;
pub use child_container::ChildContainer;
pub use component::Component;
pub use context::Context;
pub use element::{Element, NativeElement, ComponentElement, TextElement, WidgetPayload};
pub use error::{RosaceError, RosaceResult};
pub use error_boundary::ErrorBoundary;
pub use platform::{use_platform, set_platform, Platform};
pub use render_object::{AxisBound, Canvas, Constraints, RenderObject};
pub use safe_area::{use_safe_area, set_safe_area, SafeArea};
pub use semantic_node::{Role, SemanticNode};
pub use types::{AtomId, ComponentId, Key, Location, Point, Rect, Size};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::on_mount;

    struct Greeting;
    impl Component for Greeting {
        fn build(&self, _ctx: &mut Context) -> Element {
            Element::text("Hello, ROSACE!")
        }
    }

    #[test]
    fn component_builds_element() {
        let greeting = Greeting;
        let mut ctx = Context::new(ComponentId(1));
        let element = greeting.build(&mut ctx);
        assert!(!matches!(element, Element::Empty));
    }

    #[test]
    fn lifecycle_on_cleanup_registered() {
        let id = ComponentId(2);
        let mut ctx = Context::new(id);
        on_mount(&mut ctx, || || {});
        // Cleanup is stored in cleanup_store, not on Context directly.
        assert!(rosace_state::cleanup_store::has_callbacks(id));
    }

    #[test]
    fn error_boundary_has_fallback() {
        let boundary = ErrorBoundary::new()
            .fallback(|_e| Element::text("something went wrong"))
            .child(Element::text("normal content"));
        let result = boundary.render();
        assert!(!matches!(result, Element::Empty));
    }

    struct SimpleContainer { elements: Vec<Element> }
    impl SimpleContainer {
        fn new() -> Self { SimpleContainer { elements: Vec::new() } }
    }
    impl ChildContainer for SimpleContainer {
        fn child(mut self, element: impl Into<Element>) -> Self {
            self.elements.push(element.into());
            self
        }
        fn children<E: Into<Element>>(mut self, elements: Vec<E>) -> Self {
            self.elements.extend(elements.into_iter().map(|e| e.into()));
            self
        }
        fn prepend(mut self, element: impl Into<Element>) -> Self {
            self.elements.insert(0, element.into());
            self
        }
    }

    #[test]
    fn child_container_order_preserved() {
        let container = SimpleContainer::new()
            .child(Element::text("first"))
            .child(Element::text("second"))
            .child(Element::text("third"));
        assert_eq!(container.elements.len(), 3);
        let texts: Vec<&str> = container.elements.iter().filter_map(|e| {
            if let Element::Text(t) = e { Some(t.content.as_str()) } else { None }
        }).collect();
        assert_eq!(texts, ["first", "second", "third"]);
    }

    #[test]
    fn constraints_loose_has_zero_min() {
        let c = Constraints::loose(800.0, 600.0);
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.min_height, 0.0);
    }

    #[test]
    fn rosace_error_display() {
        let e = RosaceError::not_found("User");
        assert!(e.to_string().contains("User"));
    }

    #[test]
    fn context_state_creates_atom() {
        let mut ctx = Context::new(ComponentId(100));
        let atom = ctx.state(42i32);
        assert_eq!(atom.get(), 42);
        atom.set(100);
        assert_eq!(atom.get(), 100);
    }

    #[test]
    fn context_state_persists_across_frames() {
        let mut ctx = Context::new(ComponentId(200));
        let atom = ctx.state(0i32);
        atom.set(7);

        // Simulate next frame: new Context with same component_id
        let mut ctx2 = Context::new(ComponentId(200));
        let atom2 = ctx2.state(0i32);
        assert_eq!(atom2.get(), 7, "state must survive frame rebuild");
    }
}
