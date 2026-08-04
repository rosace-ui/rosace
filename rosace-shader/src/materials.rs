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
pub const GLASS_MATERIAL:    PipelineId = PipelineId::builtin(0x103);

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

// ── 4. Liquid glass — real backdrop refraction ──────────────────────────────
//
// Promoted from `examples/src/bin/liquid_glass_app.rs` (the tuned WGSL is
// unchanged) so any app can reach for it without copy-pasting a shader —
// same "curated starter, raw registry stays open underneath" reasoning as
// the other three materials above. Requires [`ShaderSpec::with_backdrop`]
// (already applied by [`register_glass_material`]) — this samples what's
// BEHIND the surface it's painted on, unlike gradient/noise/glow which are
// self-contained. Deliberately no `.fallback(..)` (see [`glass`]'s doc):
// an opaque fallback would itself get sampled by the shader on the CPU/web
// path, which is worse than the surface's own normal (pre-material)
// rendering showing through instead.

#[derive(ShaderUniforms)]
struct GlassUniforms {
    radius: f32,
    refract_px: f32,
    frost_px: f32,
    bright: f32,
}

const GLASS_WGSL: &str = r#"
struct Mat { radius: f32, refract_px: f32, frost_px: f32, bright: f32, };
@group(0) @binding(1) var<uniform> m: Mat;

fn sd_rrect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let size = rosace_quad.size_px;
    let px = in.uv * size;
    let p = px - size * 0.5;
    let d = sd_rrect(p, size * 0.5 - vec2<f32>(1.0, 1.0), m.radius);
    let mask = clamp(0.5 - d, 0.0, 1.0);

    // Thick-slab refraction: bend the sample OUTWARD near the rim, like
    // looking through the beveled edge of real glass. Quadratic falloff so
    // the panel center stays optically flat.
    let edge = 22.0;
    let bend = pow(smoothstep(-edge, 0.0, d), 2.0);
    let dir = p / max(length(p), 0.001);
    let uv_off = dir * bend * m.refract_px / size;

    // Subtle chromatic split on the refracted rim (real dispersion).
    let ca = 1.0 + bend * 0.06;
    var col: vec3<f32>;
    col.r = rosace_sample_backdrop(in.uv + uv_off * ca).r;
    col.g = rosace_sample_backdrop(in.uv + uv_off).g;
    col.b = rosace_sample_backdrop(in.uv + uv_off / ca).b;

    // Light frost: 4 extra taps in a cross — enough to soften what's
    // behind so controls stay legible, nowhere near a real gaussian cost.
    let fr = vec2<f32>(m.frost_px, m.frost_px) / size;
    var acc = col * 0.40;
    acc += rosace_sample_backdrop(in.uv + uv_off + vec2<f32>(fr.x, 0.0)).rgb * 0.15;
    acc += rosace_sample_backdrop(in.uv + uv_off - vec2<f32>(fr.x, 0.0)).rgb * 0.15;
    acc += rosace_sample_backdrop(in.uv + uv_off + vec2<f32>(0.0, fr.y)).rgb * 0.15;
    acc += rosace_sample_backdrop(in.uv + uv_off - vec2<f32>(0.0, fr.y)).rgb * 0.15;

    // Smoked-glass body: tint toward a deep neutral so light-theme text on
    // top stays legible over ANY backdrop, then a gentle lift.
    var glass = mix(acc, vec3<f32>(0.030, 0.036, 0.070), 0.38) * m.bright;
    glass += vec3<f32>(0.012);

    // Specular rim — strongest along the top edge, fading down the sides.
    let rim = smoothstep(-3.0, -0.5, d);
    glass += rim * (0.08 + 0.22 * (1.0 - in.uv.y));

    // Soft inner shade toward the bottom edge for depth.
    glass -= smoothstep(-6.0, -0.5, d) * in.uv.y * 0.04;

    // No branch anywhere: texture sampling stays in uniform control flow,
    // and outside the mask this is premultiplied transparent black.
    return vec4<f32>(glass * mask, mask);
}
"#;

/// Register the glass pipeline (idempotent — re-registration replaces).
/// Called by [`register_starter_materials`]; call directly if you only
/// want this one. Unlike the other three starters, this needs
/// [`ShaderSpec::with_backdrop`] — applied here, not left to the caller.
pub fn register_glass_material() {
    register_shader(GLASS_MATERIAL, ShaderSpec::new(GLASS_WGSL).with_backdrop());
}

/// Real backdrop-sampling liquid glass: thick-slab edge refraction, subtle
/// chromatic split, light frost, specular rim. `radius` should match the
/// surface's own corner radius (mismatched radii show a visible seam
/// between the shader's rounded mask and the widget's own clip/border).
/// Requires [`register_glass_material`] (or [`register_starter_materials`])
/// once at startup. No fallback color — see this section's module doc.
pub fn glass(radius: f32) -> ShaderMaterial {
    let u = GlassUniforms { radius, refract_px: 20.0, frost_px: 3.5, bright: 1.0 };
    ShaderMaterial::new(GLASS_MATERIAL, u.to_bytes())
}

// ── Bulk registration ───────────────────────────────────────────────────────

/// Register every starter-library pipeline at once (idempotent). Call at
/// app startup before the first frame that uses any of them.
pub fn register_starter_materials() {
    register_gradient_material();
    register_noise_material();
    register_glow_material();
    register_glass_material();
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
        assert!(ids.contains(&GLASS_MATERIAL.raw()));
    }

    #[test]
    fn glass_material_has_no_fallback() {
        let m = glass(26.0);
        assert_eq!(m.pipeline, GLASS_MATERIAL);
        assert_eq!(m.fallback, None, "an opaque fallback would itself get sampled by the shader");
    }
}
