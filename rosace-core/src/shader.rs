//! Shader uniform trait (D109/Phase 27).
//!
//! Lives in `rosace-core` (Layer 2) — not `rosace-shader` (Layer 5) — so
//! `rosace-render` (Layer 4) can produce uniform bytes for the built-in
//! GPU shape pipelines without an upward dependency. `rosace-shader`
//! re-exports it; app code sees it as `rosace::shader::ShaderUniforms`.

/// Produces this value's bytes in WGSL *uniform address space* layout
/// (scalar align 4; `vec2<f32>` align 8; `vec3<f32>`/`vec4<f32>`/`mat4x4`
/// align 16; total size rounded up to 16).
///
/// Do not implement by hand — `#[derive(ShaderUniforms)]` (`rosace-macros`)
/// generates the packing with compile-time-checked field order, alignment
/// padding, and supported-type enforcement. A hand-rolled impl that gets
/// padding wrong produces garbage uniforms with no error at any stage.
pub trait ShaderUniforms {
    fn to_bytes(&self) -> Vec<u8>;
}
