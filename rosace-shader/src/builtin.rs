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

/// The glass text-selection magnifier lens (D124 follow-up — the Phase 28
/// Step 7 magnifier, landed as a theme-driven selection style). A
/// backdrop-sampling pill that MAGNIFIES the scene under it — painted by
/// `TextInput` over an active selection when the theme's `SelectionStyle`
/// is the glass kind. Registered here (framework built-in, id inside the
/// reserved range) because a theme-driven widget behavior must not depend
/// on the app remembering to register a pipeline.
pub const SELECTION_LENS: PipelineId = PipelineId::builtin(0x20);

/// Uniforms: `radius` = pill corner radius px, `zoom` = magnification
/// (1.0 = none). All-scalar row so widget code can pack bytes without a
/// derive. Output is premultiplied; outside the pill it is transparent.
const SELECTION_LENS_WGSL: &str = r#"
struct Mat { radius: f32, zoom: f32, p2: f32, p3: f32, };
@group(0) @binding(1) var<uniform> m: Mat;

fn sd_rrect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let size = rosace_quad.size_px;
    let p = in.uv * size - size * 0.5;
    let d = sd_rrect(p, size * 0.5 - vec2<f32>(0.5, 0.5), m.radius);
    let mask = clamp(0.5 - d, 0.0, 1.0);

    // Magnify: sample a smaller window about the lens center.
    let src = vec2<f32>(0.5, 0.5) + (in.uv - vec2<f32>(0.5, 0.5)) / max(m.zoom, 1.0);
    var col = rosace_sample_backdrop(src).rgb;

    // NO body lift at all — the lens shows the magnified content exactly
    // as-is. Every earlier "subtle glass lift" here compounded with a
    // bright backdrop into washed-out glyphs; contrast inside the pill is
    // the SelectionStyle highlight band's job, not the shader's.

    // Specular rim, brighter along the top of the pill — the one glass
    // cue the lens keeps, and what makes the zoom bubble read as an
    // object floating above the text.
    let rim = smoothstep(-1.8, -0.4, d);
    col += rim * (0.10 + 0.16 * (1.0 - in.uv.y));

    return vec4<f32>(col * mask, mask);
}
"#;

/// Register every built-in shape pipeline (idempotent — re-registration
/// replaces). The platform calls this automatically when GPU shapes are
/// enabled; apps may also call it at startup before `App::run`.
pub fn register_builtins() {
    for (id, wgsl) in rosace_render::gpu_shapes::builtin_wgsl_sources() {
        register_shader(PipelineId::builtin(id), ShaderSpec::new(wgsl));
    }
    register_shader(SELECTION_LENS, ShaderSpec::new(SELECTION_LENS_WGSL).with_backdrop());
}
