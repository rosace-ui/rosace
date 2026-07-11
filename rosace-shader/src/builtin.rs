//! Built-in GPU shape pipelines — registration + typed ids (D109/Phase 27).
//!
//! The geometry conversions, uniform layout, and WGSL sources live in
//! `rosace_render::gpu_shapes` (Layer 4, so `play_picture` can use them);
//! this module is their Layer-5 typed face: `PipelineId` constants,
//! `register_builtins()`, and re-exports so app code sees one coherent
//! `rosace::shader::builtin` module.

use crate::{register_shader, PipelineId, ShaderSpec};

pub use rosace_render::gpu_shapes::{
    arc_quad, fill_rrect_quad, gradient_quad, linear_rgba, shadow_quad,
    stroke_rrect_quad, BuiltinShapeUniforms,
};

pub const FILL_RRECT:   PipelineId = PipelineId::builtin(rosace_render::gpu_shapes::FILL_RRECT_ID);
pub const STROKE_RRECT: PipelineId = PipelineId::builtin(rosace_render::gpu_shapes::STROKE_RRECT_ID);
pub const GRADIENT:     PipelineId = PipelineId::builtin(rosace_render::gpu_shapes::GRADIENT_ID);
pub const ARC:          PipelineId = PipelineId::builtin(rosace_render::gpu_shapes::ARC_ID);
pub const SHADOW:       PipelineId = PipelineId::builtin(rosace_render::gpu_shapes::SHADOW_ID);

/// Register every built-in shape pipeline (idempotent — re-registration
/// replaces). The platform calls this automatically when GPU shapes are
/// enabled; apps may also call it at startup before `App::run`.
pub fn register_builtins() {
    for (id, wgsl) in rosace_render::gpu_shapes::builtin_wgsl_sources() {
        register_shader(PipelineId::builtin(id), ShaderSpec::new(wgsl));
    }
}
