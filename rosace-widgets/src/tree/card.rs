use rosace_core::types::Size;
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::padding::EdgeInsets;
use super::container::draw_rounded_rect_pub;
use super::material::{resolve_material, CardMaterial};

/// An elevated surface — background + rounded corners + optional shadow.
///
/// The most common surface for grouping content (task card, profile card, etc.).
pub struct Card {
    pub background: Color,
    pub border_color: Option<Color>,
    pub radius: f32,
    pub elevation: f32,
    pub padding: EdgeInsets,
    pub width: Option<f32>,
    pub material: Option<ShaderMaterial>,
    pub child: BoxedWidget,
}

impl Card {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            background: Color::rgba(0, 0, 0, 0), // sentinel: use theme.surface_variant
            border_color: Some(Color::rgba(0, 0, 0, 0)), // sentinel: use theme.outline
            radius: 8.0,
            elevation: 4.0,
            padding: EdgeInsets::all(12.0),
            width: None,
            material: None,
            child: Box::new(child),
        }
    }

    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn border(mut self, c: Color) -> Self { self.border_color = Some(c); self }
    pub fn no_border(mut self) -> Self { self.border_color = None; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn elevation(mut self, e: f32) -> Self { self.elevation = e; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    /// Per-instance shader material — replaces the background fill when
    /// resolved. Beats the theme's `CardMaterial` default. Corners are drawn
    /// square under the shader (no rounded-clip primitive yet, D124 Step 4+).
    pub fn material(mut self, m: ShaderMaterial) -> Self { self.material = Some(m); self }
}

impl Widget for Card {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        // A fixed width bounds the child too (same rule as Container).
        let avail_w = self.width.unwrap_or_else(|| constraints.max_width_f32());
        let inner_c = Constraints::loose(
            (avail_w - self.padding.total_h()).max(0.0),
            (constraints.max_height_f32() - self.padding.total_v()).max(0.0),
        );
        let child_size = self.child.layout(&ctx.with_constraints(inner_c));
        let total = self.padding.grow(child_size);
        constraints.constrain(Size {
            width:  self.width.unwrap_or(total.width),
            height: total.height,
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;

        if self.elevation > 0.5 {
            ctx.fill_shadow_rrect(r, self.radius, Color::rgba(0, 0, 0, 80), self.elevation);
        }

        let material = resolve_material::<CardMaterial>(&ctx.theme, self.material.as_ref());
        if let Some(m) = &material {
            // Only paint a fallback the material EXPLICITLY carries (same
            // rule as Container). Painting one unconditionally broke
            // backdrop-sampling glass materials: the opaque rect landed in
            // the scene right before the shader quad, so the glass sampled
            // the fallback instead of the real content behind the card.
            if let Some(fallback) = m.fallback {
                draw_rounded_rect_pub(ctx, r, fallback, self.radius);
            }
            ctx.shader_fill(r, m.pipeline, m.uniforms.clone());
        } else {
            let bg = if self.background.a == 0 {
                ctx.tc(ctx.theme.colors.surface_variant)
            } else {
                self.background
            };
            draw_rounded_rect_pub(ctx, r, bg, self.radius);
        }

        if let Some(bc) = self.border_color {
            let bc = if bc.a == 0 { ctx.tc(ctx.theme.colors.outline) } else { bc };
            ctx.stroke_rrect(r, self.radius, bc, 1.0);
        }

        // Child
        let inner = self.padding.shrink(r);
        self.child.paint(&mut ctx.child(inner));
    }
}

#[cfg(test)]
mod material_cascade_tests {
    use super::*;
    use rosace_shader::PipelineId;
    use rosace_core::types::{Point, Rect, Size};

    fn mat(id: u64) -> ShaderMaterial {
        ShaderMaterial::new(PipelineId::user(0x3000 + id), vec![id as u8])
    }

    fn paint_and_check(card: Card, theme: rosace_theme::ThemeData) -> bool {
        let font = rosace_render::FontCache::embedded();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let rect = Rect {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size { width: 100.0, height: 100.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        card.paint(&mut ctx);
        let picture = recorder.finish();
        picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. }))
    }

    #[test]
    fn instance_material_paints_shader_fill() {
        let theme = rosace_theme::built_in::dark_theme();
        assert!(paint_and_check(Card::new(super::super::spacer::Spacer::new(0.0)).material(mat(1)), theme));
    }

    #[test]
    fn theme_material_used_when_no_instance() {
        let theme = rosace_theme::built_in::dark_theme().with_ext(super::super::material::CardMaterial(mat(2)));
        assert!(paint_and_check(Card::new(super::super::spacer::Spacer::new(0.0)), theme));
    }

    #[test]
    fn no_material_renders_as_before() {
        let theme = rosace_theme::built_in::dark_theme();
        assert!(!paint_and_check(Card::new(super::super::spacer::Spacer::new(0.0)), theme));
    }
}
