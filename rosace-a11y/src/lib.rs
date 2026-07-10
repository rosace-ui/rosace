//! Accessibility semantic tree for ROSACE.
//!
//! Provides `A11yTree`, `A11yNode`, `FocusManager`, and `Role` —
//! a data model for screen-reader integration and keyboard navigation.
//! Platform AT-SPI/UIA bindings are deferred to v1.0.
//!
//! # Example
//! ```rust,ignore
//! use rosace_a11y::{A11yTree, A11yNode, Role, FocusManager};
//!
//! let mut tree = A11yTree::new(0);
//! let root = A11yNode::new(0, Role::Dialog).with_label("Settings");
//! tree.add_node(root);
//! tree.add_child(0, A11yNode::new(1, Role::Button).with_label("Save"));
//! tree.add_child(0, A11yNode::new(2, Role::Button).with_label("Cancel"));
//!
//! let mut focus = FocusManager::new();
//! focus.sync(&tree);
//! assert_eq!(focus.focus_next(), Some(1));
//! assert_eq!(focus.focus_next(), Some(2));
//! assert_eq!(focus.focus_next(), Some(1)); // wraps
//! ```

pub mod focus;
pub mod focus_node;
pub mod node;
pub mod role;
pub mod tree;

pub use focus::FocusManager;
pub use focus_node::FocusNode;
pub use node::A11yNode;
pub use role::Role;
pub use tree::A11yTree;
