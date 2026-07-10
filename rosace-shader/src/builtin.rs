//! Built-in GPU shape pipelines (D109 / Phase 27 Step 3).
//!
//! The SDF fragment shaders that replace `tiny-skia`'s CPU rasterization
//! for ROSACE's built-in `DrawCommand` shapes. Five pipelines cover all
//! eight shape variants:
//!
//! | Pipeline        | Serves                                          |
//! |-----------------|-------------------------------------------------|
//! | `FILL_RRECT`    | `FillRect` (r=0), `FillRRect`, `FillCircle`     |
//! | `STROKE_RRECT`  | `StrokeRect` (r=0), `StrokeRRect`               |
//! | `GRADIENT`      | `FillGradient` (two-stop, axis-aligned, rounded)|
//! | `ARC`           | `FillArc` (ring segment, round caps)            |
//! | `SHADOW`        | `DrawShadow` (Gaussian-approx rounded shadow)   |
//!
//! Every conversion function here maps a shape in PHYSICAL pixels to
//! `(quad_rect, uniform_bytes)`: the quad is the shape's bounds inflated by
//! the AA/blur margin (an SDF's anti-aliasing ramp extends past the exact
//! bounds; an un-inflated quad would slice the ramp off), and the uniforms
//! carry the true geometry in quad-local px. Colors are sRGB u8 (what
//! `DrawCommand` records) converted to LINEAR premultiplied-ready f32 here
//! — the fragment outputs linear, the sRGB surface encodes on write (the
//! same one-correct-round-trip rule as the compositor's texture formats).
//!
//! All uniform structs share ONE layout (`BuiltinShapeUniforms`, 64 bytes)
//! so every pipeline binds identically; each interprets `params` its own
//! way, documented per conversion function.

use crate::{register_shader, PipelineId, ShaderSpec, ShaderUniforms};
use rosace_macros::ShaderUniforms;

pub const FILL_RRECT:   PipelineId = PipelineId::builtin(1);
pub const STROKE_RRECT: PipelineId = PipelineId::builtin(2);
pub const GRADIENT:     PipelineId = PipelineId::builtin(3);
pub const ARC:          PipelineId = PipelineId::builtin(4);
pub const SHADOW:       PipelineId = PipelineId::builtin(5);

/// The one uniform layout every built-in pipeline binds.
///
/// All geometry is in the RECORDING's units (quad-local), with `quad`
/// carrying the quad's size in those same units. The shader computes
/// `px_scale = rosace_quad.size_px / quad` and scales everything itself —
/// so a conversion done at logical px (widget recording, scaled ×DPR at
/// replay) and one done at physical px (scale 1:1) are BOTH correct, and
/// HiDPI can never desynchronize uniforms from quad placement.
#[derive(ShaderUniforms)]
pub struct BuiltinShapeUniforms {
    /// Primary color, LINEAR straight-alpha RGBA.
    pub color:  [f32; 4],
    /// Secondary color (gradient `to`), LINEAR straight-alpha RGBA.
    pub color2: [f32; 4],
    /// Shape geometry in quad-local units: rects (x, y, w, h); arcs
    /// (center_x, center_y, unused, unused).
    pub shape:  [f32; 4],
    /// Pipeline-specific parameters — see each conversion fn.
    pub params: [f32; 4],
    /// The quad's (w, h) in the same units as `shape`/`params` lengths.
    pub quad:   [f32; 2],
}

/// sRGB u8 → linear f32, the exact EOTF (not the 2.2 shortcut) — this is
/// the inverse of what the sRGB swapchain applies on write, so a shader
/// fill of `Color::rgb(43,45,48)` lands at (43,45,48), byte-identical to
/// the CPU path (the 2026-07-08 double-gamma bug is the cautionary tale).
fn srgb_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// sRGB u8 RGBA → linear f32 RGBA (alpha is linear already).
pub fn linear_rgba(rgba: [u8; 4]) -> [f32; 4] {
    [
        srgb_to_linear(rgba[0]),
        srgb_to_linear(rgba[1]),
        srgb_to_linear(rgba[2]),
        rgba[3] as f32 / 255.0,
    ]
}

/// AA margin: the SDF coverage ramp is 1px wide, centered on the edge.
const AA_MARGIN: f32 = 1.0;

/// A quad rect `(x, y, w, h)` inflated by `m` on every side.
fn inflate(rect: (f32, f32, f32, f32), m: f32) -> (f32, f32, f32, f32) {
    (rect.0 - m, rect.1 - m, rect.2 + 2.0 * m, rect.3 + 2.0 * m)
}

/// `FillRect`/`FillRRect`/`FillCircle` → `FILL_RRECT` quad.
/// params: (corner_radius, 0, 0, 0).
pub fn fill_rrect_quad(
    rect: (f32, f32, f32, f32), radius: f32, rgba: [u8; 4],
) -> ((f32, f32, f32, f32), Vec<u8>) {
    let quad = inflate(rect, AA_MARGIN);
    let r = radius.max(0.0).min(rect.2 / 2.0).min(rect.3 / 2.0);
    let u = BuiltinShapeUniforms {
        color:  linear_rgba(rgba),
        color2: [0.0; 4],
        shape:  [rect.0 - quad.0, rect.1 - quad.1, rect.2, rect.3],
        params: [r, 0.0, 0.0, 0.0],
        quad:   [quad.2, quad.3],
    };
    (quad, u.to_bytes())
}

/// `StrokeRect`/`StrokeRRect` → `STROKE_RRECT` quad. The stroke is centered
/// on the shape edge (tiny-skia `Stroke` convention).
/// params: (corner_radius, stroke_width, 0, 0).
pub fn stroke_rrect_quad(
    rect: (f32, f32, f32, f32), radius: f32, width: f32, rgba: [u8; 4],
) -> ((f32, f32, f32, f32), Vec<u8>) {
    let quad = inflate(rect, AA_MARGIN + width / 2.0);
    let r = radius.max(0.0).min(rect.2 / 2.0).min(rect.3 / 2.0);
    let u = BuiltinShapeUniforms {
        color:  linear_rgba(rgba),
        color2: [0.0; 4],
        shape:  [rect.0 - quad.0, rect.1 - quad.1, rect.2, rect.3],
        params: [r, width, 0.0, 0.0],
        quad:   [quad.2, quad.3],
    };
    (quad, u.to_bytes())
}

/// `FillGradient` → `GRADIENT` quad. Two stops, `from` at the rect's
/// top/left edge to `to` at the bottom/right (pad spread), masked by the
/// rounded rect. params: (corner_radius, vertical ? 1 : 0, 0, 0).
///
/// Colors are passed as sRGB (NOT pre-linearized like every other
/// pipeline): tiny-skia interpolates gradient stops in sRGB space, so the
/// shader must mix in sRGB and linearize AFTER — verified by A/B midpoint
/// sampling (linear-space mixing measured +16/255 red at the midpoint of
/// the violet→blue reference gradient vs the CPU path).
pub fn gradient_quad(
    rect: (f32, f32, f32, f32), radius: f32, from: [u8; 4], to: [u8; 4], vertical: bool,
) -> ((f32, f32, f32, f32), Vec<u8>) {
    let quad = inflate(rect, AA_MARGIN);
    let r = radius.max(0.0).min(rect.2 / 2.0).min(rect.3 / 2.0);
    let srgb = |c: [u8; 4]| [
        c[0] as f32 / 255.0, c[1] as f32 / 255.0,
        c[2] as f32 / 255.0, c[3] as f32 / 255.0,
    ];
    let u = BuiltinShapeUniforms {
        color:  srgb(from),
        color2: srgb(to),
        shape:  [rect.0 - quad.0, rect.1 - quad.1, rect.2, rect.3],
        params: [r, if vertical { 1.0 } else { 0.0 }, 0.0, 0.0],
        quad:   [quad.2, quad.3],
    };
    (quad, u.to_bytes())
}

/// `FillArc` → `ARC` quad: ring segment of `thickness` along the circle of
/// `radius` centered at `center`, from `start_deg` sweeping `sweep_deg`
/// clockwise (0° = 3 o'clock, y-down), ROUND caps (the CPU path strokes
/// with `LineCap::Round`). shape: (center in quad-local px);
/// params: (radius, thickness, start_rad, sweep_rad) — sweep normalized
/// non-negative here so the shader needs no sign handling.
pub fn arc_quad(
    center: (f32, f32), radius: f32, thickness: f32,
    start_deg: f32, sweep_deg: f32, rgba: [u8; 4],
) -> ((f32, f32, f32, f32), Vec<u8>) {
    let reach = radius + thickness / 2.0;
    let quad = inflate(
        (center.0 - reach, center.1 - reach, reach * 2.0, reach * 2.0),
        AA_MARGIN,
    );
    let (start, sweep) = if sweep_deg < 0.0 {
        (start_deg + sweep_deg, -sweep_deg)
    } else {
        (start_deg, sweep_deg)
    };
    let u = BuiltinShapeUniforms {
        color:  linear_rgba(rgba),
        color2: [0.0; 4],
        shape:  [center.0 - quad.0, center.1 - quad.1, 0.0, 0.0],
        params: [radius, thickness, start.to_radians(), sweep.min(360.0).to_radians()],
        quad:   [quad.2, quad.3],
    };
    (quad, u.to_bytes())
}

/// Blur margin multiplier: the visible falloff of the CPU path's
/// triple-box-blur mask extends roughly 1.5×blur past the rect (its mask
/// allocates `margin = blur` on each side plus the AA edge); 2× is safely
/// past visually-zero for the Gaussian approximation too.
const SHADOW_MARGIN: f32 = 2.0;

/// `DrawShadow` → `SHADOW` quad: Gaussian-approximate drop shadow of the
/// rounded rect. params: (corner_radius, sigma, 0, 0). Sigma maps from the
/// CPU path's box-blur `blur` parameter: three box passes of width b
/// approximate a Gaussian with σ ≈ b/2 — tuned against the real
/// `build_shadow_mask` output in the A/B demo, not derived on paper.
pub fn shadow_quad(
    rect: (f32, f32, f32, f32), radius: f32, blur: f32, rgba: [u8; 4],
) -> ((f32, f32, f32, f32), Vec<u8>) {
    let quad = inflate(rect, AA_MARGIN + blur.max(0.0) * SHADOW_MARGIN);
    let r = radius.max(0.0).min(rect.2 / 2.0).min(rect.3 / 2.0);
    let u = BuiltinShapeUniforms {
        color:  linear_rgba(rgba),
        color2: [0.0; 4],
        shape:  [rect.0 - quad.0, rect.1 - quad.1, rect.2, rect.3],
        params: [r, (blur * 0.5).max(0.25), 0.0, 0.0],
        quad:   [quad.2, quad.3],
    };
    (quad, u.to_bytes())
}

/// Shared WGSL: uniform struct + SDF library, prepended to every built-in
/// fragment. (The framework's vertex stage + `rosace_quad` come from the
/// compositor's own header — see shader_quad_header.wgsl.)
const SDF_LIB: &str = r#"
struct BuiltinShapeUniforms {
    color:  vec4<f32>,
    color2: vec4<f32>,
    shape:  vec4<f32>,
    params: vec4<f32>,
    quad:   vec2<f32>,
};
@group(0) @binding(1) var<uniform> u: BuiltinShapeUniforms;

// Signed distance to a rounded rect centered at the origin with half-size
// `half` and corner radius `r`. Negative inside.
fn sd_rrect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// 1px-wide coverage ramp centered on the edge (d in px).
fn aa_cov(d: f32) -> f32 {
    return clamp(0.5 - d, 0.0, 1.0);
}

// Quad-local position in physical px.
fn local_px(uv: vec2<f32>) -> vec2<f32> {
    return uv * rosace_quad.size_px;
}

// Recording-units -> physical-px scale (the DPR when uniforms were built
// from logical px; 1.0 when built from physical px). x == y in practice.
fn px_scale() -> vec2<f32> {
    return rosace_quad.size_px / max(u.quad, vec2<f32>(1e-6, 1e-6));
}

// Premultiply a straight-alpha linear color by coverage.
fn out_color(c: vec4<f32>, cov: f32) -> vec4<f32> {
    let a = c.a * cov;
    return vec4<f32>(c.rgb * a, a);
}
"#;

const FILL_RRECT_FS: &str = r#"
@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let sc = px_scale();
    let p = local_px(in.uv) - (u.shape.xy + u.shape.zw * 0.5) * sc;
    let d = sd_rrect(p, u.shape.zw * 0.5 * sc, u.params.x * sc.x);
    return out_color(u.color, aa_cov(d));
}
"#;

const STROKE_RRECT_FS: &str = r#"
@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let sc = px_scale();
    let p = local_px(in.uv) - (u.shape.xy + u.shape.zw * 0.5) * sc;
    let d = abs(sd_rrect(p, u.shape.zw * 0.5 * sc, u.params.x * sc.x)) - u.params.y * sc.x * 0.5;
    return out_color(u.color, aa_cov(d));
}
"#;

const GRADIENT_FS: &str = r#"
@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let sc = px_scale();
    let lp = local_px(in.uv) - u.shape.xy * sc;
    var t: f32;
    if u.params.y > 0.5 {
        t = clamp(lp.y / max(u.shape.w * sc.y, 1e-6), 0.0, 1.0);
    } else {
        t = clamp(lp.x / max(u.shape.z * sc.x, 1e-6), 0.0, 1.0);
    }
    // Mix in sRGB (tiny-skia's convention), then linearize for output —
    // the surface re-encodes to sRGB on write.
    let c_srgb = mix(u.color, u.color2, t);
    let lo = c_srgb.rgb / 12.92;
    let hi = pow((c_srgb.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let c = vec4<f32>(select(hi, lo, c_srgb.rgb <= vec3<f32>(0.04045)), c_srgb.a);
    let p = local_px(in.uv) - (u.shape.xy + u.shape.zw * 0.5) * sc;
    let d = sd_rrect(p, u.shape.zw * 0.5 * sc, u.params.x * sc.x);
    return out_color(c, aa_cov(d));
}
"#;

const ARC_FS: &str = r#"
const TAU: f32 = 6.28318530718;

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let sc = px_scale();
    let p = local_px(in.uv) - u.shape.xy * sc;
    let radius = u.params.x * sc.x;
    let start  = u.params.z;
    let sweep  = u.params.w;

    // Angle of this pixel, wrapped relative to the arc start.
    var rel = atan2(p.y, p.x) - start;
    rel = rel - floor(rel / TAU) * TAU; // wrap to [0, TAU)

    var d: f32;
    if rel <= sweep {
        // Within the swept angle: distance to the arc's centerline circle.
        d = abs(length(p) - radius);
    } else {
        // Outside: distance to the nearer endpoint — round caps for free.
        let e0 = vec2<f32>(cos(start), sin(start)) * radius;
        let a1 = start + sweep;
        let e1 = vec2<f32>(cos(a1), sin(a1)) * radius;
        d = min(distance(p, e0), distance(p, e1));
    }
    return out_color(u.color, aa_cov(d - u.params.y * sc.x * 0.5));
}
"#;

const SHADOW_FS: &str = r#"
// Gaussian CDF via an Abramowitz-Stegun-style erf approximation — the
// rounded-rect SDF pushed through the CDF gives the blurred coverage
// (exact along straight edges, slightly tighter than a true 2D blur at
// corners; visually verified against the CPU box-blur mask in the A/B
// demo).
fn erf_approx(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    var t = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
    t = t * t;
    return s - s / (t * t);
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let sc = px_scale();
    let p = local_px(in.uv) - (u.shape.xy + u.shape.zw * 0.5) * sc;
    let d = sd_rrect(p, u.shape.zw * 0.5 * sc, u.params.x * sc.x);
    let sigma = max(u.params.y * sc.x, 0.25);
    let cov = 0.5 - 0.5 * erf_approx(d / (sigma * 1.41421356));
    return out_color(u.color, cov);
}
"#;

/// Register every built-in shape pipeline (idempotent — re-registration
/// replaces). Called by the app/platform once at startup, BEFORE `App::run`,
/// so the compositor compiles them eagerly with everything else.
pub fn register_builtins() {
    let src = |fs: &str| ShaderSpec::new(format!("{SDF_LIB}\n{fs}"));
    register_shader(FILL_RRECT,   src(FILL_RRECT_FS));
    register_shader(STROKE_RRECT, src(STROKE_RRECT_FS));
    register_shader(GRADIENT,     src(GRADIENT_FS));
    register_shader(ARC,          src(ARC_FS));
    register_shader(SHADOW,       src(SHADOW_FS));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_quad_inflates_by_aa_margin_and_offsets_shape_into_quad_space() {
        let (quad, bytes) = fill_rrect_quad((10.0, 20.0, 100.0, 50.0), 8.0, [255, 0, 0, 255]);
        assert_eq!(quad, (9.0, 19.0, 102.0, 52.0));
        assert_eq!(bytes.len(), 80, "4×vec4 + vec2 quad size, rounded to 16");
        // shape.xy = true rect origin in quad-local px = (1, 1).
        assert_eq!(&bytes[32..40], &[1.0f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat()[..]);
    }

    #[test]
    fn stroke_quad_margin_covers_half_the_stroke_width() {
        let (quad, _) = stroke_rrect_quad((100.0, 100.0, 50.0, 50.0), 0.0, 6.0, [0, 0, 0, 255]);
        // Inflation = 1 (AA) + 3 (width/2) = 4 per side.
        assert_eq!(quad, (96.0, 96.0, 58.0, 58.0));
    }

    #[test]
    fn negative_sweep_normalizes_to_positive_from_shifted_start() {
        let (_, bytes) = arc_quad((50.0, 50.0), 20.0, 4.0, 90.0, -90.0, [0, 0, 0, 255]);
        let start = f32::from_le_bytes(bytes[56..60].try_into().unwrap());
        let sweep = f32::from_le_bytes(bytes[60..64].try_into().unwrap());
        assert!((start - 0.0f32.to_radians()).abs() < 1e-6, "start must shift back: {start}");
        assert!((sweep - 90.0f32.to_radians()).abs() < 1e-6, "sweep must be positive: {sweep}");
    }

    #[test]
    fn srgb_conversion_round_trips_the_known_gamma_bug_color() {
        // The D109 gamma discipline test color: #2B2D30 must come back as
        // itself after linear → sRGB-surface encode. Linearizing 43/255
        // then re-encoding must round-trip to 43.
        let lin = srgb_to_linear(43);
        let re = if lin <= 0.0031308 { lin * 12.92 } else { 1.055 * lin.powf(1.0 / 2.4) - 0.055 };
        assert_eq!((re * 255.0).round() as u8, 43);
    }

    #[test]
    fn radius_clamps_to_half_extent_like_the_cpu_path() {
        let (_, bytes) = fill_rrect_quad((0.0, 0.0, 20.0, 10.0), 99.0, [0, 0, 0, 255]);
        let r = f32::from_le_bytes(bytes[48..52].try_into().unwrap());
        assert_eq!(r, 5.0, "radius must clamp to min(w,h)/2");
    }
}
