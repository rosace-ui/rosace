use std::sync::Arc;

use crate::component::Component;
use crate::types::{ComponentId, Key};

/// Type-erased bridge between the element tree and the render layer.
///
/// Implemented by `WidgetBox` in `rosace-widgets`. Defined here in
/// `rosace-core` so `NativeElement` can hold it without a circular dep.
pub trait WidgetPayload: Send + Sync + 'static {
    /// Returns `self` as `&dyn Any` so the render walker can downcast.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// An element representing a component instance in the tree.
#[derive(Clone)]
pub struct ComponentElement {
    pub id: ComponentId,
    pub key: Option<Key>,
    /// The component that produced this element. The walker calls `build()` on it.
    pub component: Arc<dyn Component>,
    pub children: Vec<Element>,
}

/// An element backed by a native widget (a `Box<dyn Widget>`).
#[derive(Clone)]
pub struct NativeElement {
    /// Debug label (type name of the widget).
    pub tag: &'static str,
    /// The actual widget, type-erased. Walker downcasts to `WidgetBox`.
    /// `None` for element-tree-only nodes (e.g. layout-crate containers).
    pub payload: Option<Arc<dyn WidgetPayload>>,
    pub children: Vec<Element>,
    /// Optional stable key for reconciler identity (local to sibling list).
    pub key: Option<Key>,
}

/// A plain text leaf node.
#[derive(Clone)]
pub struct TextElement {
    pub content: String,
}

/// The fundamental unit of the ROSACE element tree.
///
/// Elements are lightweight descriptions of what to render. `Component::build()`
/// returns an `Element`; the framework walks the tree to produce pixels.
#[derive(Clone)]
pub enum Element {
    Component(ComponentElement),
    Native(NativeElement),
    Text(TextElement),
    Empty,
}

impl Element {
    pub fn empty() -> Self { Element::Empty }

    pub fn text(content: impl Into<String>) -> Self {
        Element::Text(TextElement { content: content.into() })
    }

    /// Attach a stable reconciler key to this element (local to sibling list).
    pub fn with_key(self, key: impl Into<Key>) -> Self {
        match self {
            Element::Native(mut n) => { n.key = Some(key.into()); Element::Native(n) }
            Element::Component(mut c) => { c.key = Some(key.into()); Element::Component(c) }
            other => other,
        }
    }
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Element::Component(c) => write!(f, "Component(id={})", c.id.0),
            Element::Native(n)    => write!(f, "Native({})", n.tag),
            Element::Text(t)      => write!(f, "Text({:?})", t.content),
            Element::Empty        => write!(f, "Empty"),
        }
    }
}
