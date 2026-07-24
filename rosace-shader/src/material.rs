//! `ShaderMaterial` (D124/Phase 33) — the one value type that carries a
//! custom material through every level of the cascade: a per-widget
//! `.material(...)` override, an app-wide theme-extension default, or a
//! `ShaderPaint` widget's own content.
//!
//! Deliberately tiny and `Clone + PartialEq`: theme extensions are stored
//! and compared by value (D105), and a widget rebuilt every frame clones
//! its resolved material cheaply — the `uniforms` byte vec is the only
//! heap cost, and it's small (one uniform buffer's worth).

use rosace_render::Color;
use crate::PipelineId;

/// A resolved material: which registered pipeline to draw with, the uniform
/// bytes to feed it, and an honest CPU/web fallback.
///
/// Produced via [`ShaderMaterial::new`] + a `#[derive(ShaderUniforms)]`
/// struct's `to_bytes()`, or one of the ready-made constructors in
/// [`crate::materials`]. The `pipeline` must already be registered
/// ([`crate::register_shader`]) — a material only names a pipeline, it does
/// not own or compile it, so the same pipeline is shared by every material/
/// widget that references its id (the "register once, use everywhere" story).
#[derive(Clone, PartialEq, Debug)]
pub struct ShaderMaterial {
    pub pipeline: PipelineId,
    pub uniforms: Vec<u8>,
    /// Painted by the ordinary fill path when there is NO GPU compositor
    /// (desktop softbuffer fallback, web's putImageData) — `ShaderFill`
    /// quads are dropped on those paths (see `SkiaCanvas`'s non-`gpu_shapes`
    /// branch), so without this a material would render nothing there.
    /// `None` ⇒ the widget's own normal rendering shows instead (as if no
    /// material was set) — never a silent hole.
    pub fallback: Option<Color>,
}

impl ShaderMaterial {
    /// A material drawing with `pipeline`, fed `uniforms` (normally
    /// `my_uniform_struct.to_bytes()`).
    pub fn new(pipeline: PipelineId, uniforms: Vec<u8>) -> Self {
        Self { pipeline, uniforms, fallback: None }
    }

    /// The solid color shown where the GPU shader path is unavailable
    /// (CPU/web). Strongly recommended for any material used on a real
    /// surface widget — it is the difference between "degrades to a flat
    /// color" and "vanishes" on those targets.
    pub fn fallback(mut self, color: Color) -> Self {
        self.fallback = Some(color);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_fallback_by_default() {
        let m = ShaderMaterial::new(PipelineId::user(0x300), vec![1, 2, 3, 4]);
        assert_eq!(m.pipeline, PipelineId::user(0x300));
        assert_eq!(m.uniforms, vec![1, 2, 3, 4]);
        assert!(m.fallback.is_none());
    }

    #[test]
    fn fallback_builder_sets_color_and_is_value_comparable() {
        let a = ShaderMaterial::new(PipelineId::user(0x300), vec![]).fallback(Color::rgb(10, 20, 30));
        let b = ShaderMaterial::new(PipelineId::user(0x300), vec![]).fallback(Color::rgb(10, 20, 30));
        assert_eq!(a, b, "materials are compared by value (theme-ext requirement)");
        assert_eq!(a.fallback, Some(Color::rgb(10, 20, 30)));
    }
}
