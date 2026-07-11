//! Custom-shader showcase (D109): effects tiny-skia's vocabulary could
//! never express, registered as app-authored WGSL pipelines.
//!
//! - LIQUID GLASS: a translucent panel over colorful content — animated
//!   refraction wobble, fresnel rim light, a drifting specular streak.
//!   (True backdrop BLUR needs the deferred two-pass compositor follow-up;
//!   this is everything glass can be with forward alpha blending.)
//! - MORPH: a signed-distance blob cycling circle → star → rounded square,
//!   with an outer glow.
//!
//! Both animate via a `time` uniform — uniforms are what the compositor's
//! skip-present diffing watches, so a time-driven shader MUST take its
//! clock as data (the frame-skip contract, PHASE_27.md).

use rosace::prelude::*;
use rosace::shader::{register_shader, PipelineId, ShaderSpec, ShaderUniforms};
use rosace::widgets::tree::{anim_clock, request_animation};

#[derive(ShaderUniforms)]
struct FxUniforms {
    time: f32,
}

const GLASS_WGSL: &str = r#"
struct FxUniforms { time: vec4<f32> };
@group(0) @binding(1) var<uniform> u: FxUniforms;

fn sd_rrect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let t = u.time.x;
    let px = in.uv * rosace_quad.size_px;
    let half = rosace_quad.size_px * 0.5 - vec2<f32>(2.0, 2.0);
    let p = px - rosace_quad.size_px * 0.5;
    let d = sd_rrect(p, half, 26.0);
    let mask = clamp(0.5 - d, 0.0, 1.0);
    if mask <= 0.0 { return vec4<f32>(0.0); }

    // Refraction wobble: two crossed sine ripples pretending to be a
    // liquid surface normal.
    let ripple = sin(px.x * 0.045 + t * 1.7) * cos(px.y * 0.038 - t * 1.3)
               + 0.5 * sin((px.x + px.y) * 0.02 + t * 0.9);

    // Base glass tint — cool, faint, brighter where the "liquid" bulges.
    var col = vec3<f32>(0.75, 0.85, 1.0) * (0.10 + 0.05 * ripple);
    var alpha = 0.22 + 0.05 * ripple;

    // Fresnel rim: glass edges catch light.
    let rim = smoothstep(-14.0, -1.0, d);
    col += vec3<f32>(0.9, 0.95, 1.0) * rim * 0.5;
    alpha += rim * 0.35;

    // Drifting specular streak (a diagonal band sweeping with time).
    let band = (px.x + px.y * 0.6) / (rosace_quad.size_px.x + rosace_quad.size_px.y * 0.6);
    let sweep = fract(band - t * 0.15);
    let streak = smoothstep(0.46, 0.5, sweep) * (1.0 - smoothstep(0.5, 0.54, sweep));
    col += vec3<f32>(1.0, 1.0, 1.0) * streak * 0.55;
    alpha += streak * 0.25;

    alpha = clamp(alpha, 0.0, 0.92) * mask;
    return vec4<f32>(col * alpha, alpha);
}
"#;

const MORPH_WGSL: &str = r#"
struct FxUniforms { time: vec4<f32> };
@group(0) @binding(1) var<uniform> u: FxUniforms;

fn sd_circle(p: vec2<f32>, r: f32) -> f32 { return length(p) - r; }

fn sd_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// 5-point star: radial distance modulated by angle.
fn sd_star(p: vec2<f32>, r: f32, t: f32) -> f32 {
    let ang = atan2(p.y, p.x) + t * 0.6; // slow spin
    let m = 0.72 + 0.28 * cos(ang * 5.0);
    return length(p) - r * m;
}

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let t = u.time.x;
    let px = in.uv * rosace_quad.size_px;
    let p = px - rosace_quad.size_px * 0.5;
    let r = min(rosace_quad.size_px.x, rosace_quad.size_px.y) * 0.32;

    // Cycle circle -> star -> rounded square -> circle, 2s per morph.
    let phase = t / 2.0;
    let seg = fract(phase);
    let k = smoothstep(0.15, 0.85, seg); // ease within each morph
    let which = i32(floor(phase)) % 3;

    let dc = sd_circle(p, r);
    let ds = sd_star(p, r * 1.15, t);
    let db = sd_box(p, vec2<f32>(r, r), r * 0.3);
    var d: f32;
    if which == 0 {
        d = mix(dc, ds, k);
    } else if which == 1 {
        d = mix(ds, db, k);
    } else {
        d = mix(db, dc, k);
    }

    // Fill with a slowly hue-shifting color + soft outer glow.
    let hue = vec3<f32>(
        0.6 + 0.4 * sin(t * 0.7),
        0.4 + 0.3 * sin(t * 0.9 + 2.1),
        0.9 + 0.1 * sin(t * 0.5 + 4.2),
    );
    let fill = clamp(0.5 - d, 0.0, 1.0);
    let glow = exp(-max(d, 0.0) * 0.06) * 0.35;
    let a = clamp(fill + glow, 0.0, 1.0);
    let col = hue * (fill + glow * 1.6);
    return vec4<f32>(col * a, a); // premultiplied
}
"#;

fn glass_pipeline() -> PipelineId { PipelineId::user(0x2000) }
fn morph_pipeline() -> PipelineId { PipelineId::user(0x2001) }

struct FxDemo;

impl Component for FxDemo {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            CustomPaint::new(|cx, size| {
                // Keep the frame loop ticking; time flows in as a uniform.
                request_animation();
                let t = anim_clock();
                let uniforms = FxUniforms { time: t }.to_bytes();
                let ox = cx.rect.origin.x;
                let oy = cx.rect.origin.y;

                // ── Colorful content BEHIND the glass, so translucency is
                // provable: gradient card, circles, text.
                cx.record(rosace::render::DrawCommand::FillGradient {
                    rect: r(ox + 30.0, oy + 30.0, 420.0, 250.0),
                    radius: 18.0,
                    from: Color { r: 187, g: 134, b: 252, a: 255 },
                    to: Color { r: 30, g: 90, b: 220, a: 255 },
                    vertical: false,
                });
                for i in 0..5 {
                    cx.fill_circle(
                        Point { x: ox + 70.0 + i as f32 * 85.0, y: oy + 155.0 },
                        26.0,
                        Color { r: 255, g: 180 - i as u8 * 30, b: 90 + i as u8 * 30, a: 255 },
                    );
                }
                cx.text("content behind the glass", 60.0, 60.0, Color::WHITE, 20.0);

                // ── LIQUID GLASS panel overlapping that content.
                cx.shader_fill(r(ox + 110.0, oy + 90.0, 300.0, 150.0), glass_pipeline(), uniforms.clone());

                // ── MORPH blob on its own patch.
                cx.text("SDF morph: circle → star → square", 490.0, 60.0, Color::WHITE, 14.0);
                cx.shader_fill(r(ox + 480.0, oy + 80.0, 220.0, 220.0), morph_pipeline(), uniforms);

                let _ = size;
            }),
        )
        .app_bar(AppBar::new("Custom shaders — liquid glass + morph"))
        .into_element()
    }
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
}

fn main() {
    env_logger::init();
    register_shader(glass_pipeline(), ShaderSpec::new(GLASS_WGSL));
    register_shader(morph_pipeline(), ShaderSpec::new(MORPH_WGSL));

    App::new()
        .title("shader_effects_demo")
        .size(760, 400)
        .launch(FxDemo);
}
