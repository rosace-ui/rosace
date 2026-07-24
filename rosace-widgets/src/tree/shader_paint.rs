//! `ShaderPaint` (D124/Phase 33) — a leaf widget that fills its rect with a
//! registered custom shader [`ShaderMaterial`].
//!
//! This is the widget D109 promised ("a `ShaderPaint` widget, own type, not
//! a `CustomPaint` mode-switch") and never landed. It is a thin, honest
//! layer over the already-shipped `PaintCtx::shader_fill` — the registry,
//! the `DrawCommand::ShaderFill` plumbing, and the compositor's eager
//! pipeline compilation all predate it (D109).
//!
//! **Rectangular by design.** The widget fills a plain rect; any rounded/
//! shaped output is the material's own fragment shader's job (there is no
//! rounded-clip primitive in the pipeline — the starter `glow` material,
//! for instance, does its own radial falloff). Wiring a material as a
//! *rounded* surface background is the `Container`/`Card` `.material()`
//! path (Phase 33 Step 3), not this widget.
//!
//! **Decorative by default.** No semantics entry and no hit region — it is
//! paint, not a control. Wrap it in a `Pressable`/`Button` if you need
//! interaction.
//!
//! ```rust,ignore
//! // once at startup:
//! rosace_shader::materials::register_starter_materials();
//! // in build():
//! ShaderPaint::new(rosace_shader::materials::gradient(a, b, 0.6, 0.3))
//!     .size(200.0, 120.0)
//!     .animated()
//! ```

use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx};

pub struct ShaderPaint {
    material: ShaderMaterial,
    width: Option<f32>,
    height: Option<f32>,
    /// When set, the material's standard `time` uniform slot (byte offset 0,
    /// see `rosace_shader::materials::patch_time`) is advanced from the
    /// animation clock every frame, and the widget requests the next frame —
    /// EVENT-driven, honoring the D123 "no free-running loops" rule (frames
    /// are requested, not spun).
    animated: bool,
}

impl ShaderPaint {
    pub fn new(material: ShaderMaterial) -> Self {
        Self { material, width: None, height: None, animated: false }
    }

    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = Some(h);
        self
    }

    /// Drive the material's `time` uniform from a live clock (for the
    /// starter animated materials — gradient flow, grain, glow pulse). No
    /// effect on a material whose shader ignores `time`.
    ///
    /// GPU-resident (D109 maturity, 2026-07-18): this no longer repaints
    /// the widget per frame — the quad is recorded ONCE with
    /// `animate_time`, and the platform patches the time uniform straight
    /// into the cached GPU quad at every present. Continuous animation
    /// costs a 16-byte buffer write per frame, not a CPU tree repaint
    /// (which at full-window size was a real 110%-CPU debug-build loop).
    pub fn animated(mut self) -> Self { self.animated = true; self }
}

impl Widget for ShaderPaint {
    fn layout(&self, ctx: &LayoutCtx) -> rosace_core::types::Size {
        let c = ctx.constraints;
        let w = self.width.unwrap_or_else(|| c.max_width_f32());
        let h = self.height.unwrap_or_else(|| c.max_height_f32());
        c.constrain(rosace_core::types::Size {
            width:  if w.is_finite() { w } else { 0.0 },
            height: if h.is_finite() { h } else { 0.0 },
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;

        // Honest CPU/web degradation: paint the fallback color FIRST (a
        // normal fill). On the GPU path the shader quad below covers it; on
        // softbuffer/web (where `ShaderFill` is dropped) it is what remains.
        // Opaque materials set a fallback and it's invisible on GPU;
        // translucent ones (e.g. glow) set None and nothing is painted here.
        if let Some(fallback) = self.material.fallback {
            ctx.fill_rect(rect, fallback);
        }

        if self.animated {
            ctx.shader_fill_animated(rect, self.material.pipeline, self.material.uniforms.clone());
        } else {
            ctx.shader_fill(rect, self.material.pipeline, self.material.uniforms.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;
    use rosace_render::Color;
    use rosace_shader::PipelineId;

    fn test_material() -> ShaderMaterial {
        ShaderMaterial::new(PipelineId::user(0x1000), vec![0u8; 16]).fallback(Color::rgb(20, 20, 40))
    }

    #[test]
    fn explicit_size_is_honored() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = ShaderPaint::new(test_material()).size(200.0, 120.0).layout(&ctx);
        assert_eq!((size.width, size.height), (200.0, 120.0));
    }

    #[test]
    fn unsized_fills_available_bounded_space() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(300.0, 250.0), &font, &theme);
        let size = ShaderPaint::new(test_material()).layout(&ctx);
        assert_eq!((size.width, size.height), (300.0, 250.0));
    }

    #[test]
    fn paint_records_a_shader_fill_command() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let rect = rosace_core::types::Rect {
            origin: rosace_core::types::Point { x: 0.0, y: 0.0 },
            size: rosace_core::types::Size { width: 100.0, height: 100.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        ShaderPaint::new(test_material()).paint(&mut ctx);
        let picture = recorder.finish();
        // A fallback fill + the shader fill both recorded.
        let has_shader = picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. }));
        assert!(has_shader, "ShaderPaint must record a ShaderFill draw command");
    }
}
