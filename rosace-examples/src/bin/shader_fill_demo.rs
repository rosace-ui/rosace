//! Phase 27 Step 2 exit-bar demo (D109): one trivial registered shader
//! rendered via a raw `DrawCommand::ShaderFill` — no widget wrapper —
//! pixel-verified end to end: record → registry → compositor → present.
//!
//! The fragment fills the rect with a uniform-supplied color, split into a
//! left/right half-and-half so the verification can prove the uniform bytes
//! AND the uv coordinate both arrived intact (a solid fill could pass with
//! a broken vertex stage).

use rosace::prelude::*;
use rosace::shader::{register_shader, PipelineId, ShaderSpec, ShaderUniforms};

/// Uniform layout must mirror the WGSL struct below: two vec4 colors.
#[derive(ShaderUniforms)]
struct SplitColors {
    left:  [f32; 4],
    right: [f32; 4],
}

/// Fragment-only WGSL — the framework prepends the quad vertex stage and
/// `RosaceVsOut`/`rosace_quad` (see rosace-compositor/shader_quad_header.wgsl).
/// Output is premultiplied linear color.
const SPLIT_WGSL: &str = r#"
struct SplitColors {
    left:  vec4<f32>,
    right: vec4<f32>,
};
@group(0) @binding(1) var<uniform> u: SplitColors;

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let c = select(u.right, u.left, in.uv.x < 0.5);
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;

fn split_pipeline() -> PipelineId {
    PipelineId::user(0x1000)
}

struct ShaderFillDemo;

impl Component for ShaderFillDemo {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 40.0))
                .child(Text::new("GPU ShaderFill — left green, right blue").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 20.0))
                .child(
                    // Raw command via CustomPaint (D100) — deliberately NOT a
                    // dedicated shader widget; that's Step 5, out of scope here.
                    CustomPaint::new(|cx, size| {
                        let rect = Rect { origin: cx.rect.origin, size };
                        let uniforms = SplitColors {
                            left:  [0.0, 1.0, 0.0, 1.0], // pure green
                            right: [0.0, 0.0, 1.0, 1.0], // pure blue
                        }
                        .to_bytes();
                        cx.shader_fill(rect, split_pipeline(), uniforms);
                    })
                    .size(320.0, 160.0),
                ),
        )
        .app_bar(AppBar::new("shader_fill_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    // Eager registration (D109): queued now, compiled by the platform at
    // startup — before the first frame that references it.
    register_shader(split_pipeline(), ShaderSpec::new(SPLIT_WGSL));

    App::new()
        .title("ShaderFill demo")
        .size(480, 320)
        .launch(ShaderFillDemo);
}
