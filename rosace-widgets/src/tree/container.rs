use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::{Color, DrawCommand};
use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};
use super::padding::EdgeInsets;
use super::material::{resolve_material, ContainerMaterial};

/// Box shape (D095 — a circle is a Container, not a CircleWidget).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BoxShape {
    /// Rounded rect using `radius` (0 = sharp corners).
    #[default]
    Rect,
    /// Fully circular (radius = min(w, h) / 2).
    Circle,
    /// Pill: radius = height / 2.
    Stadium,
}

/// The most fundamental building block — a maximally-configurable box:
/// shape, background (solid or gradient), border, shadow, corner radius,
/// padding, margin, fixed/min size, alignment, child clipping, and a child.
///
/// Everything box-shaped is a `Container` — there is no ColoredBox / CircleBox
/// / GradientBox (D095). Analogous to a CSS `div` or Flutter's `Container`.
pub struct Container {
    pub background: Option<Color>,
    pub gradient: Option<(Color, Color, bool)>, // (from, to, vertical)
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub border_radius: f32,
    pub shape: BoxShape,
    pub shadow_blur: f32,
    pub shadow_color: Color,
    pub padding: EdgeInsets,
    pub margin: EdgeInsets,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: f32,
    pub min_height: f32,
    pub clip: bool,
    pub align: Option<super::Alignment>,
    pub material: Option<ShaderMaterial>,
    pub child: Option<BoxedWidget>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            background: None,
            gradient: None,
            border_color: None,
            border_width: 1.0,
            border_radius: 0.0,
            shape: BoxShape::Rect,
            shadow_blur: 0.0,
            shadow_color: Color::rgba(0, 0, 0, 0),
            padding: EdgeInsets::default(),
            margin: EdgeInsets::default(),
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            clip: false,
            align: None,
            material: None,
            child: None,
        }
    }

    /// Effective corner radius given the shape and box size.
    fn radius_for(&self, size: Size) -> f32 {
        match self.shape {
            BoxShape::Rect    => self.border_radius,
            BoxShape::Circle  => size.width.min(size.height) / 2.0,
            BoxShape::Stadium => size.height / 2.0,
        }
    }

    pub fn align(mut self, a: super::Alignment) -> Self { self.align = Some(a); self }
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Two-stop linear gradient background (overrides solid `background`).
    pub fn gradient(mut self, from: Color, to: Color) -> Self { self.gradient = Some((from, to, true)); self }
    pub fn gradient_h(mut self, from: Color, to: Color) -> Self { self.gradient = Some((from, to, false)); self }
    pub fn border(mut self, c: Color, w: f32) -> Self { self.border_color = Some(c); self.border_width = w; self }
    pub fn radius(mut self, r: f32) -> Self { self.border_radius = r; self }
    pub fn shape(mut self, s: BoxShape) -> Self { self.shape = s; self }
    pub fn circle(mut self) -> Self { self.shape = BoxShape::Circle; self }
    pub fn stadium(mut self) -> Self { self.shape = BoxShape::Stadium; self }
    pub fn shadow(mut self, color: Color, blur: f32) -> Self { self.shadow_color = color; self.shadow_blur = blur; self }
    /// Material-style elevation shortcut (black shadow scaled by elevation).
    pub fn elevation(mut self, e: f32) -> Self { self.shadow_color = Color::rgba(0, 0, 0, 90); self.shadow_blur = e; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn margin(mut self, m: EdgeInsets) -> Self { self.margin = m; self }
    /// Clip the child to the box shape (rounded/circle content masking).
    pub fn clip(mut self) -> Self { self.clip = true; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn size(mut self, w: f32, h: f32) -> Self { self.width = Some(w); self.height = Some(h); self }
    pub fn min_size(mut self, w: f32, h: f32) -> Self { self.min_width = w; self.min_height = h; self }
    pub fn child(mut self, w: impl Widget + 'static) -> Self { self.child = Some(Box::new(w)); self }
    /// Per-instance shader material — replaces the background fill (gradient/
    /// solid) when resolved. Beats the theme's `ContainerMaterial` default.
    /// Corners are drawn square under the shader (no rounded-clip primitive
    /// yet, D124 Step 4+); border/shadow/child/radius are unaffected.
    pub fn material(mut self, m: ShaderMaterial) -> Self { self.material = Some(m); self }
}

impl Default for Container {
    fn default() -> Self { Self::new() }
}

impl Widget for Container {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let child_size = self.child.as_ref().map(|c| {
            // A fixed width/height bounds the CHILD too — a 240px-wide card's
            // text must wrap at 240px even when the parent offers infinity.
            let avail_w = self.width.unwrap_or_else(|| avail_w(constraints));
            let avail_h = self.height.unwrap_or_else(|| avail_h(constraints));
            let inner_c = Constraints::loose(
                (avail_w - self.padding.total_h()).max(0.0),
                (avail_h - self.padding.total_v()).max(0.0),
            );
            self.padding.grow(c.layout(&ctx.with_constraints(inner_c)))
        }).unwrap_or(Size { width: 0.0, height: 0.0 });

        // With an alignment set, fill the available (bounded) space —
        // Flutter semantics; a shrink-wrapped box has no room to align in.
        let (fill_w, fill_h) = if self.align.is_some() {
            (avail_w(constraints), avail_h(constraints))
        } else {
            (f32::INFINITY, f32::INFINITY) // sentinel: not used below
        };
        let w = self.width.unwrap_or_else(|| {
            if self.align.is_some() && fill_w.is_finite() { fill_w }
            else { child_size.width.max(self.min_width) }
        });
        let h = self.height.unwrap_or_else(|| {
            if self.align.is_some() && fill_h.is_finite() { fill_h }
            else { child_size.height.max(self.min_height) }
        });

        // Margin is added around the box — it occupies more layout space but
        // the visual box (bg/border/child) is inset by the margin at paint.
        constraints.constrain(Size {
            width:  w.max(self.min_width) + self.margin.total_h(),
            height: h.max(self.min_height) + self.margin.total_v(),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Inset by margin: the box draws inside its allocated rect.
        let rect = self.margin.shrink(ctx.rect);
        let radius = self.radius_for(rect.size);

        // Drop shadow — source shape matches the (possibly rounded) box.
        if self.shadow_blur > 0.5 {
            ctx.fill_shadow_rrect(rect, radius, self.shadow_color, self.shadow_blur);
        }

        // Background — material (instance, else theme default) wins over
        // gradient/solid; gradient wins over solid.
        let material = resolve_material::<ContainerMaterial>(&ctx.theme, self.material.as_ref());
        if let Some(m) = material {
            if let Some(fallback) = m.fallback {
                if radius > 0.5 { ctx.fill_rrect(rect, radius, fallback); }
                else { ctx.fill_rect(rect, fallback); }
            }
            ctx.shader_fill(rect, m.pipeline, m.uniforms);
        } else if let Some((from, to, vertical)) = self.gradient {
            ctx.fill_gradient(rect, radius, from, to, vertical);
        } else if let Some(bg) = self.background {
            if radius > 0.5 { ctx.fill_rrect(rect, radius, bg); }
            else { ctx.fill_rect(rect, bg); }
        }

        // Border — same corner geometry as the background.
        if let Some(bc) = self.border_color {
            if radius > 0.5 { ctx.stroke_rrect(rect, radius, bc, self.border_width); }
            else { ctx.stroke_rect(rect, bc, self.border_width); }
        }

        // Child — optionally clipped to the box, aligned or filling padded rect.
        if let Some(child) = &self.child {
            let inner = self.padding.shrink(rect);
            let child_rect = if let Some(align) = self.align {
                let inner_c = Constraints::loose(inner.size.width, inner.size.height);
                let child_size = child.layout(&ctx.layout_ctx(inner_c));
                let off = align.offset(inner.size, child_size);
                Rect {
                    origin: Point { x: inner.origin.x + off.x, y: inner.origin.y + off.y },
                    size: child_size,
                }
            } else {
                inner
            };
            if self.clip {
                ctx.record(DrawCommand::PushClip { rect });
                child.paint(&mut ctx.child(child_rect));
                ctx.record(DrawCommand::PopClip);
            } else {
                child.paint(&mut ctx.child(child_rect));
            }
        }
    }
}

/// Fill a rounded rectangle through a `PaintCtx` (used by widgets that need
/// rounded corners but aren't `Container`).
pub(super) fn draw_rounded_rect_pub(ctx: &mut PaintCtx, rect: Rect, color: Color, radius: f32) {
    ctx.fill_rrect(rect, radius, color);
}

#[cfg(test)]
mod material_cascade_tests {
    use super::*;
    use rosace_shader::PipelineId;

    fn mat(id: u64) -> ShaderMaterial {
        ShaderMaterial::new(PipelineId::user(0x2000 + id), vec![id as u8])
    }

    fn paint_and_check(container: Container, theme: rosace_theme::ThemeData) -> bool {
        let font = rosace_render::FontCache::embedded();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let rect = Rect {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size { width: 100.0, height: 100.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        container.paint(&mut ctx);
        let picture = recorder.finish();
        picture.commands.iter().any(|c| matches!(c, DrawCommand::ShaderFill { .. }))
    }

    #[test]
    fn instance_material_paints_shader_fill() {
        let theme = rosace_theme::built_in::dark_theme();
        assert!(paint_and_check(Container::new().material(mat(1)), theme));
    }

    #[test]
    fn theme_material_used_when_no_instance() {
        let theme = rosace_theme::built_in::dark_theme().with_ext(super::super::material::ContainerMaterial(mat(2)));
        assert!(paint_and_check(Container::new(), theme));
    }

    #[test]
    fn no_material_renders_as_before() {
        let theme = rosace_theme::built_in::dark_theme();
        assert!(!paint_and_check(Container::new().background(Color::rgb(10, 10, 10)), theme));
    }
}
