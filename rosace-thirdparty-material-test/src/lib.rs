//! Proof crate for D124 Phase 33 Step 5's extensibility bar (mirrors D115
//! Step 2's icon-registry proof): a pipeline, a `MaterialKey`, and a widget
//! defined ENTIRELY outside `rosace-*` — depending only on the public
//! `rosace` umbrella crate, exactly like a real end-user app would.
//!
//! No `rosace-widgets`/`rosace-shader`/`rosace-compositor` source is edited
//! to make this work — everything here goes through the already-public
//! material cascade (`MaterialKey`/`resolve_material`) and shader registry
//! (`register_shader`/`PipelineId::user`).

use rosace::shader::{register_shader, PipelineId, ShaderSpec, ShaderMaterial, ShaderUniforms};
use rosace::widgets::tree::{LayoutCtx, PaintCtx, Widget, resolve_material, MaterialKey};
use rosace::Size;

/// Plain sRGB-byte -> linear-float conversion — a third party has no access
/// to `rosace-render`'s internal gamma helpers, so it does its own (a real
/// pipeline author would ship a proper sRGB curve; this is demo-grade).
fn linear(c: rosace::Color) -> [f32; 4] {
    let f = |b: u8| (b as f32 / 255.0).powf(2.2);
    [f(c.r), f(c.g), f(c.b), c.a as f32 / 255.0]
}

/// A third-party pipeline id — user range, picked independently of anything
/// in rosace-shader's starter library.
pub fn stripes_pipeline() -> PipelineId {
    PipelineId::user(0x9000)
}

#[derive(ShaderUniforms)]
struct StripesUniforms {
    color_a: [f32; 4],
    color_b: [f32; 4],
}

const STRIPES_WGSL: &str = r#"
struct Mat { color_a: vec4<f32>, color_b: vec4<f32>, };
@group(0) @binding(1) var<uniform> m: Mat;

@fragment
fn fs_main(in: RosaceVsOut) -> @location(0) vec4<f32> {
    let stripe = floor((in.uv.x + in.uv.y) * 10.0) % 2.0;
    return select(m.color_a, m.color_b, stripe > 0.5);
}
"#;

/// Registers the third-party pipeline. A real app would call this once at
/// startup, same as `rosace::shader::materials::register_starter_materials`.
pub fn register_stripes_pipeline() {
    register_shader(stripes_pipeline(), ShaderSpec::new(STRIPES_WGSL));
}

/// A diagonal two-color stripe material.
pub fn stripes(a: rosace::Color, b: rosace::Color) -> ShaderMaterial {
    let u = StripesUniforms {
        color_a: linear(a),
        color_b: linear(b),
    };
    ShaderMaterial::new(stripes_pipeline(), u.to_bytes()).fallback(a)
}

/// The third-party widget's OWN theme-default material key — a brand-new
/// type, not one of rosace-widgets' `ContainerMaterial`/`CardMaterial`/etc.
/// Proves a widget author can add their OWN slot to the cascade.
pub struct StripesPanelMaterial(pub ShaderMaterial);
impl MaterialKey for StripesPanelMaterial {
    fn material(&self) -> &ShaderMaterial { &self.0 }
}

/// A minimal third-party surface widget: fixed size, paints its resolved
/// material (instance override, else the theme's `StripesPanelMaterial`,
/// else nothing) — the exact `instance -> theme -> none` cascade shape
/// every rosace-widgets surface widget uses, reimplemented here with zero
/// access to rosace-widgets internals.
pub struct StripesPanel {
    material: Option<ShaderMaterial>,
    width: f32,
    height: f32,
}

impl StripesPanel {
    pub fn new() -> Self {
        Self { material: None, width: 120.0, height: 80.0 }
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
    pub fn material(mut self, m: ShaderMaterial) -> Self {
        self.material = Some(m);
        self
    }
}

impl Default for StripesPanel {
    fn default() -> Self { Self::new() }
}

impl Widget for StripesPanel {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.width, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        let material = resolve_material::<StripesPanelMaterial>(&ctx.theme, self.material.as_ref());
        if let Some(m) = material {
            if let Some(fallback) = m.fallback {
                ctx.fill_rect(rect, fallback);
            }
            ctx.shader_fill(rect, m.pipeline, m.uniforms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_material_paints_a_shader_fill() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(
            rosace::widgets::tree::render_tree::RenderTree::new(),
        ));
        let rect = rosace::Rect {
            origin: rosace::Point { x: 0.0, y: 0.0 },
            size: Size { width: 120.0, height: 80.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        let m = stripes(rosace::Color::rgb(200, 60, 60), rosace::Color::rgb(60, 60, 200));
        StripesPanel::new().material(m).paint(&mut ctx);
        let picture = recorder.finish();
        assert!(
            picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. })),
            "third-party widget must record a ShaderFill without touching rosace-widgets source"
        );
    }

    #[test]
    fn theme_default_material_applies_with_no_instance_override() {
        let m = stripes(rosace::Color::rgb(10, 200, 90), rosace::Color::rgb(90, 10, 200));
        let theme = rosace_theme::built_in::dark_theme().with_ext(StripesPanelMaterial(m));

        let font = rosace_render::FontCache::embedded();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(
            rosace::widgets::tree::render_tree::RenderTree::new(),
        ));
        let rect = rosace::Rect {
            origin: rosace::Point { x: 0.0, y: 0.0 },
            size: Size { width: 120.0, height: 80.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        StripesPanel::new().paint(&mut ctx);
        let picture = recorder.finish();
        assert!(
            picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. })),
            "the theme-registered third-party material must apply with no per-instance override"
        );
    }

    #[test]
    fn no_material_anywhere_paints_nothing() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(
            rosace::widgets::tree::render_tree::RenderTree::new(),
        ));
        let rect = rosace::Rect {
            origin: rosace::Point { x: 0.0, y: 0.0 },
            size: Size { width: 120.0, height: 80.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        StripesPanel::new().paint(&mut ctx);
        let picture = recorder.finish();
        assert!(picture.commands.is_empty(), "no material anywhere must render nothing, not crash");
    }
}
