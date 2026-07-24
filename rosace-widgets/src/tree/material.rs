//! The material cascade (D124/Phase 33): resolve which [`ShaderMaterial`] a
//! widget draws with, `instance → theme → none`.
//!
//! This is deliberately the SAME shape as `resolve_physics`
//! (`scroll_view.rs`) — the codebase's established precedent for
//! `explicit → theme.ext → default` resolution — just returning `Option`
//! (a widget with no material at any level renders normally, its
//! pre-material default).
//!
//! **"Global" is per widget KIND**, not one material for every surface.
//! Each material-capable widget declares its own key newtype implementing
//! [`MaterialKey`] (`CardMaterial(ShaderMaterial)`, `ContainerMaterial(..)`,
//! …) and stashes it on the theme via `ThemeData::with_ext` (D105's
//! type-keyed extension map). A `Card` can be glass while `Button`s stay
//! solid; a third-party widget defines its OWN key + material slot without
//! editing rosace — the same extensibility bar the icon registry (D115) set.

use rosace_shader::ShaderMaterial;
use rosace_theme::ThemeData;

/// A theme-extension newtype that carries a widget-kind's default material.
///
/// Implement on a `pub struct FooMaterial(pub ShaderMaterial)` for widget
/// `Foo`, register it with `theme.with_ext(FooMaterial(m))`, and `Foo`'s
/// paint resolves it via [`resolve_material`]. The newtype (not a bare
/// `ShaderMaterial`) is what type-keys the theme slot per widget-kind — two
/// widgets can hold different app-wide materials at once.
pub trait MaterialKey: std::any::Any + Send + Sync + 'static {
    fn material(&self) -> &ShaderMaterial;
}

/// Resolve the effective material for a widget: an explicit per-instance
/// `.material(...)` wins; else the theme's `K` default (if registered);
/// else `None` (render normally). Mirrors `resolve_physics`'s
/// `explicit.or_else(theme.ext)` precedent exactly.
pub fn resolve_material<K: MaterialKey>(
    theme: &ThemeData,
    explicit: Option<&ShaderMaterial>,
) -> Option<ShaderMaterial> {
    explicit
        .cloned()
        .or_else(|| theme.ext::<K>().map(|k| k.material().clone()))
}

/// `Container`'s theme-default material slot — `theme.with_ext(ContainerMaterial(m))`.
pub struct ContainerMaterial(pub ShaderMaterial);
impl MaterialKey for ContainerMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `Card`'s theme-default material slot — `theme.with_ext(CardMaterial(m))`.
pub struct CardMaterial(pub ShaderMaterial);
impl MaterialKey for CardMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `Dialog`'s theme-default material slot (D124 Phase 33 Step 5).
pub struct DialogMaterial(pub ShaderMaterial);
impl MaterialKey for DialogMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `Sheet`'s theme-default material slot (D124 Phase 33 Step 5).
pub struct SheetMaterial(pub ShaderMaterial);
impl MaterialKey for SheetMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `Drawer`'s theme-default material slot (D124 Phase 33 Step 5).
pub struct DrawerMaterial(pub ShaderMaterial);
impl MaterialKey for DrawerMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `AppBar`'s theme-default material slot (D124 Phase 33 Step 5).
pub struct AppBarMaterial(pub ShaderMaterial);
impl MaterialKey for AppBarMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// `BottomNavigationBar`'s theme-default material slot (D124 Phase 33 Step 5).
pub struct BottomNavMaterial(pub ShaderMaterial);
impl MaterialKey for BottomNavMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_shader::PipelineId;

    struct TestKey(ShaderMaterial);
    impl MaterialKey for TestKey {
        fn material(&self) -> &ShaderMaterial { &self.0 }
    }

    fn mat(id: u64) -> ShaderMaterial {
        ShaderMaterial::new(PipelineId::user(0x1000 + id), vec![id as u8])
    }

    #[test]
    fn instance_material_wins_over_theme() {
        let theme = rosace_theme::built_in::dark_theme().with_ext(TestKey(mat(1)));
        let explicit = mat(2);
        let resolved = resolve_material::<TestKey>(&theme, Some(&explicit));
        assert_eq!(resolved, Some(mat(2)), "explicit instance material must win");
    }

    #[test]
    fn theme_material_used_when_no_instance() {
        let theme = rosace_theme::built_in::dark_theme().with_ext(TestKey(mat(3)));
        let resolved = resolve_material::<TestKey>(&theme, None);
        assert_eq!(resolved, Some(mat(3)), "theme default applies with no instance override");
    }

    #[test]
    fn none_when_neither_set() {
        let theme = rosace_theme::built_in::dark_theme();
        let resolved = resolve_material::<TestKey>(&theme, None);
        assert!(resolved.is_none(), "no material anywhere → render normally");
    }

    #[test]
    fn distinct_keys_hold_independent_materials() {
        struct OtherKey(ShaderMaterial);
        impl MaterialKey for OtherKey {
            fn material(&self) -> &ShaderMaterial { &self.0 }
        }
        // Same theme, two widget-kind keys, two different materials — proves
        // "global is per widget kind", not one shader for everything.
        let theme = rosace_theme::built_in::dark_theme()
            .with_ext(TestKey(mat(5)))
            .with_ext(OtherKey(mat(6)));
        assert_eq!(resolve_material::<TestKey>(&theme, None), Some(mat(5)));
        assert_eq!(resolve_material::<OtherKey>(&theme, None), Some(mat(6)));
    }
}
