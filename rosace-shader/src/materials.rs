//! Starter material library (D124/Phase 33) — a curated set of ready-made
//! `ShaderMaterial`s so apps get expressive custom surfaces WITHOUT writing
//! any WGSL, plus the reference WGSL for authors who do (Apple ships a
//! curated declarative style, not raw shader authorship; this is the same
//! idea — the raw registry stays open underneath).
//!
//! Each material is:
//!   * a registered pipeline (`register_*_material()` / [`register_starter_materials`]),
//!   * a uniform struct following the **standard time convention** (an
//!     animated material's `time: f32` is its FIRST field, at byte offset 0,
//!     so [`patch_time`] can advance it each frame without knowing the rest
//!     of the layout), and
//!   * a convenience constructor returning a [`ShaderMaterial`].
//!
//! Ids `0x100..0x110` are reserved for this library. App/third-party
//! pipelines should use higher ids (`PipelineId::user(0x1000)`+) to stay
//! clear of it.

use rosace_render::Color;
use rosace_render::gpu_shapes::linear_rgba;
// The derive macro (macro namespace) and the trait (type namespace) share
// the name `ShaderUniforms` and BOTH must be in scope by that bare name: the
// derive's generated `impl ShaderUniforms for T` references the trait
// unqualified. They coexist because they occupy different namespaces — same
// dual-import this crate's own lib.rs tests use.
use rosace_macros::ShaderUniforms;
use crate::{register_shader, PipelineId, ShaderMaterial, ShaderSpec, ShaderUniforms};

/// Standard animated-material convention: `time: f32` lives at byte offset
/// 0 of the uniform buffer. Overwrite it in place each frame (the
/// `ShaderPaint`/material widget path does this automatically when a
/// material is `.animated()`), so animation needs no per-material knowledge
/// of the rest of the layout. No-op on a buffer shorter than 4 bytes.
pub fn patch_time(uniforms: &mut [u8], time: f32) {
    if uniforms.len() >= 4 {
        uniforms[0..4].copy_from_slice(&time.to_le_bytes());
    }
}

// ── Ids (reserved 0x100..0x110) ─────────────────────────────────────────────

pub const GRADIENT_MATERIAL: PipelineId = PipelineId::builtin(0x100);
pub const NOISE_MATERIAL:    PipelineId = PipelineId::builtin(0x101);
pub const GLOW_MATERIAL:     PipelineId = PipelineId::builtin(0x102);

// ── 1. Flowing animated linear gradient ─────────────────────────────────────

#[derive(ShaderUniforms)]
struct GradientUniforms {
    time:    f32,      // offset 0 (standard slot)
    angle:   f32,      // gradient direction, radians
    speed:   f32,      // flow speed
    color_a: [f32; 4], // linear straight-alpha
    color_b: [f32; 4],
}

const GRADIENT_WGSL: &str = r#"
struct Mat {
    time:    f32,
    angle:   f32,
    speed:   f32,
    color_a: vec4<f32>,
    color_b: vec4<f32>,
};
@group(0) @binding(1) var<uniform> m: Mat;

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let dir = vec2<f32>(cos(m.angle), sin(m.angle));
    let p   = in.uv - vec2<f32>(0.5, 0.5);
    let t   = fract(dot(p, dir) + 0.5 + m.time * m.speed);
    // Triangle wave → smooth ping-pong between the two colors, no seam.
    let tt  = 1.0 - abs(2.0 * t - 1.0);
    let c   = mix(m.color_a, m.color_b, tt);
    return vec4<f32>(c.rgb * c.a, c.a); // premultiplied linear (surface encodes sRGB)
}
"#;

/// Register the flowing-gradient pipeline (idempotent — re-registration
/// replaces). Called by [`register_starter_materials`]; call directly if
/// you only want this one.
pub fn register_gradient_material() {
    register_shader(GRADIENT_MATERIAL, ShaderSpec::new(GRADIENT_WGSL));
}

/// A flowing gradient between `a` and `b`. `angle` in radians (0 = →),
/// `speed` in cycles/sec (0 = static). Requires [`register_gradient_material`]
/// (or [`register_starter_materials`]) once at startup.
pub fn gradient(a: Color, b: Color, angle: f32, speed: f32) -> ShaderMaterial {
    let u = GradientUniforms {
        time: 0.0, angle, speed,
        color_a: linear_rgba(a.rgba_bytes()),
        color_b: linear_rgba(b.rgba_bytes()),
    };
    ShaderMaterial::new(GRADIENT_MATERIAL, u.to_bytes()).fallback(a)
}

// ── 2. Film-grain noise over a base color ───────────────────────────────────

#[derive(ShaderUniforms)]
struct NoiseUniforms {
    time:      f32,    // offset 0
    intensity: f32,    // 0..1 grain strength
    color:     [f32; 4],
}

const NOISE_WGSL: &str = r#"
struct Mat {
    time:      f32,
    intensity: f32,
    color:     vec4<f32>,
};
@group(0) @binding(1) var<uniform> m: Mat;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let px    = floor(in.uv * rosace_quad.size_px);
    let n     = hash(px + vec2<f32>(floor(m.time * 60.0), 0.0));
    let grain = (n - 0.5) * m.intensity;
    let rgb   = clamp(m.color.rgb + vec3<f32>(grain, grain, grain), vec3<f32>(0.0), vec3<f32>(1.0));
    let a     = m.color.a;
    return vec4<f32>(rgb * a, a);
}
"#;

pub fn register_noise_material() {
    register_shader(NOISE_MATERIAL, ShaderSpec::new(NOISE_WGSL));
}

/// A `color` surface with animated film grain. `intensity` 0..1. Requires
/// [`register_noise_material`] (or [`register_starter_materials`]) once.
pub fn noise(color: Color, intensity: f32) -> ShaderMaterial {
    let u = NoiseUniforms {
        time: 0.0,
        intensity: intensity.clamp(0.0, 1.0),
        color: linear_rgba(color.rgba_bytes()),
    };
    ShaderMaterial::new(NOISE_MATERIAL, u.to_bytes()).fallback(color)
}

// ── 3. Pulsing radial glow ──────────────────────────────────────────────────

#[derive(ShaderUniforms)]
struct GlowUniforms {
    time:   f32,    // offset 0
    radius: f32,    // glow radius, uv fraction
    speed:  f32,    // pulse speed, radians/sec
    color:  [f32; 4],
}

const GLOW_WGSL: &str = r#"
struct Mat {
    time:   f32,
    radius: f32,
    speed:  f32,
    color:  vec4<f32>,
};
@group(0) @binding(1) var<uniform> m: Mat;

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let d      = distance(in.uv, vec2<f32>(0.5, 0.5));
    let pulse  = 0.5 + 0.5 * sin(m.time * m.speed);
    let r      = m.radius * (0.7 + 0.3 * pulse);
    let inten  = 1.0 - smoothstep(0.0, r, d);
    let a      = m.color.a * inten;
    return vec4<f32>(m.color.rgb * a, a);
}
"#;

pub fn register_glow_material() {
    register_shader(GLOW_MATERIAL, ShaderSpec::new(GLOW_WGSL));
}

/// A radial `color` glow that pulses. `radius` in uv fraction (0.5 ≈ fills),
/// `speed` in radians/sec (0 = steady). Requires [`register_glow_material`]
/// (or [`register_starter_materials`]) once.
pub fn glow(color: Color, radius: f32, speed: f32) -> ShaderMaterial {
    let u = GlowUniforms { time: 0.0, radius, speed, color: linear_rgba(color.rgba_bytes()) };
    // No opaque fallback — a glow over nothing is nothing; let the widget's
    // own background show on CPU/web rather than a flat block.
    ShaderMaterial::new(GLOW_MATERIAL, u.to_bytes())
}

// ── Bulk registration ───────────────────────────────────────────────────────

/// Register every starter-library pipeline at once (idempotent). Call at
/// app startup before the first frame that uses any of them.
pub fn register_starter_materials() {
    register_gradient_material();
    register_noise_material();
    register_glow_material();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_time_writes_offset_zero() {
        let mut buf = vec![0u8; 16];
        patch_time(&mut buf, 3.5);
        assert_eq!(&buf[0..4], &3.5f32.to_le_bytes());
        assert_eq!(&buf[4..16], &[0u8; 12], "only the time slot changes");
    }

    #[test]
    fn patch_time_noop_on_short_buffer() {
        let mut buf = vec![0u8; 2];
        patch_time(&mut buf, 9.0); // must not panic
        assert_eq!(buf, vec![0u8; 2]);
    }

    #[test]
    fn gradient_material_time_is_at_offset_zero() {
        let m = gradient(Color::rgb(255, 0, 0), Color::rgb(0, 0, 255), 0.0, 1.0);
        assert_eq!(m.pipeline, GRADIENT_MATERIAL);
        // time defaults to 0.0 at the standard slot, patchable each frame.
        assert_eq!(&m.uniforms[0..4], &0.0f32.to_le_bytes());
        assert_eq!(m.fallback, Some(Color::rgb(255, 0, 0)));
    }

    #[test]
    fn starter_materials_register_without_panicking() {
        let _ = crate::take_pending_shaders(); // clear
        register_starter_materials();
        let drained = crate::take_pending_shaders();
        let ids: Vec<u64> = drained.iter().map(|(id, _)| id.raw()).collect();
        assert!(ids.contains(&GRADIENT_MATERIAL.raw()));
        assert!(ids.contains(&NOISE_MATERIAL.raw()));
        assert!(ids.contains(&GLOW_MATERIAL.raw()));
    }
}
