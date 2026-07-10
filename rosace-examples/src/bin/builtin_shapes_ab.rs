//! Phase 27 Step 3 parity harness (D109): every built-in shape drawn twice
//! per row — LEFT via the CPU tiny-skia path (`cx.fill_rrect` etc.), RIGHT
//! via the corresponding built-in GPU SDF pipeline — for pixel-level A/B
//! comparison before any call site is migrated. The two columns must be
//! visually indistinguishable.

use rosace::prelude::*;
use rosace::shader::builtin;

const CPU_X: f32 = 30.0;
const GPU_X: f32 = 330.0;

struct ShapesAb;

impl Component for ShapesAb {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            CustomPaint::new(|cx, _size| {
                let ox = cx.rect.origin.x;
                let oy = cx.rect.origin.y;
                let red    = Color { r: 220, g: 60,  b: 60,  a: 255 };
                let violet = Color { r: 187, g: 134, b: 252, a: 255 };
                let blue   = Color { r: 60,  g: 90,  b: 220, a: 255 };
                let white  = Color { r: 230, g: 230, b: 235, a: 255 };

                // Row 1 — FillRRect r=12.
                let y = oy + 10.0;
                cx.fill_rrect(rect(ox + CPU_X, y, 220.0, 56.0), 12.0, red);
                let (q, u) = builtin::fill_rrect_quad(
                    (ox + GPU_X, y, 220.0, 56.0), 12.0, [220, 60, 60, 255]);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::FILL_RRECT, u);

                // Row 2 — FillCircle r=28 (GPU: square rrect with r = w/2).
                let y = y + 70.0;
                cx.fill_circle(Point { x: ox + CPU_X + 28.0, y: y + 28.0 }, 28.0, violet);
                let (q, u) = builtin::fill_rrect_quad(
                    (ox + GPU_X, y, 56.0, 56.0), 28.0, [187, 134, 252, 255]);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::FILL_RRECT, u);

                // Row 3 — StrokeRRect r=10, width 4.
                let y = y + 70.0;
                cx.stroke_rect(rect(ox + CPU_X, y, 220.0, 48.0), white, 4.0);
                let (q, u) = builtin::stroke_rrect_quad(
                    (ox + GPU_X, y, 220.0, 48.0), 0.0, 4.0, [230, 230, 235, 255]);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::STROKE_RRECT, u);

                // Row 4 — FillGradient vertical violet→blue, r=10.
                let y = y + 64.0;
                cx.record(rosace::render::DrawCommand::FillGradient {
                    rect: rect(ox + CPU_X, y, 220.0, 56.0),
                    radius: 10.0, from: violet, to: blue, vertical: true,
                });
                let (q, u) = builtin::gradient_quad(
                    (ox + GPU_X, y, 220.0, 56.0), 10.0,
                    [187, 134, 252, 255], [60, 90, 220, 255], true);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::GRADIENT, u);

                // Row 5 — FillArc: 270° ring, thickness 8, radius 26.
                let y = y + 72.0;
                cx.record(rosace::render::DrawCommand::FillArc {
                    center: Point { x: ox + CPU_X + 30.0, y: y + 30.0 },
                    radius: 26.0, thickness: 8.0,
                    start_deg: -90.0, sweep_deg: 270.0, color: violet,
                });
                let (q, u) = builtin::arc_quad(
                    (ox + GPU_X + 30.0, y + 30.0), 26.0, 8.0, -90.0, 270.0,
                    [187, 134, 252, 255]);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::ARC, u);

                // Row 6 — DrawShadow r=12, blur=14 (light color so the
                // falloff is visible on the dark theme background).
                let y = y + 76.0;
                cx.record(rosace::render::DrawCommand::DrawShadow {
                    rect: rect(ox + CPU_X, y, 220.0, 48.0),
                    radius: 12.0, color: white, blur: 14.0,
                });
                let (q, u) = builtin::shadow_quad(
                    (ox + GPU_X, y, 220.0, 48.0), 12.0, 14.0, [230, 230, 235, 255]);
                cx.shader_fill(rect(q.0, q.1, q.2, q.3), builtin::SHADOW, u);
            }),
        )
        .app_bar(AppBar::new("CPU (left) vs GPU (right)"))
        .into_element()
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
}

fn main() {
    env_logger::init();
    builtin::register_builtins();

    App::new()
        .title("builtin_shapes_ab")
        .size(640, 520)
        .launch(ShapesAb);
}
