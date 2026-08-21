/// Accessibility tree, roles, and focus management (former `rosace-a11y`, D131).
pub mod a11y;
/// Localization: message bundles, locale, `t()` lookup (former `rosace-i18n`, D131).
pub mod i18n;

pub mod app_lifecycle;
pub mod asset;
pub mod context;
pub mod error;
pub mod ime_hint;
pub mod lifecycle;
pub mod media_query;
pub mod nav_back;
pub mod persist;
pub mod platform;
pub mod render_object;
pub mod safe_area;
pub mod semantic_node;
pub mod shader;
pub mod types;

pub use app_lifecycle::{app_lifecycle, set_app_lifecycle, use_app_lifecycle, LifecycleState};
pub use context::Context;
pub use error::{RosaceError, RosaceResult};
pub use ime_hint::{ime_cursor_area, keyboard_type, set_ime_cursor_area, set_keyboard_type, KeyboardType};
pub use media_query::{use_media_query, set_media_query, MediaQuery};
pub use persist::{persist_backend, set_persist_backend, PersistBackend, PersistValue};
pub use platform::{use_platform, set_platform, Platform};
pub use render_object::{AxisBound, Canvas, Constraints, RenderObject};
pub use safe_area::{use_safe_area, set_safe_area, SafeArea};
pub use semantic_node::{Role, SemanticNode};
pub use types::{AtomId, ComponentId, Key, Location, Point, Rect, Size};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::on_mount;

    #[test]
    fn lifecycle_on_cleanup_registered() {
        let id = ComponentId(2);
        let mut ctx = Context::new(id);
        on_mount(&mut ctx, || || {});
        // Cleanup is stored in cleanup_store, not on Context directly.
        assert!(rosace_state::cleanup_store::has_callbacks(id));
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
