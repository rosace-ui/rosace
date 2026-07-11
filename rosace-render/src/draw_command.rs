use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};

use crate::canvas::Color;
use crate::font::FontWeight;

/// A single drawing instruction recorded during the paint pass.
///
/// Widgets push these into a [`PictureRecorder`] instead of writing pixels
/// directly. The compositor later replays them onto whatever backend is active
/// (currently [`SkiaCanvas`], eventually wgpu).
#[derive(Debug, Clone)]
pub enum DrawCommand {
    FillRect   { rect: Rect, color: Color },
    StrokeRect { rect: Rect, color: Color, width: f32 },
    /// Filled rounded rectangle — a single anti-aliased path.
    FillRRect  { rect: Rect, radius: f32, color: Color },
    /// Rounded rectangle outline — matches the FillRRect geometry so borders
    /// hug rounded fills instead of framing them with square corners.
    StrokeRRect { rect: Rect, radius: f32, color: Color, width: f32 },
    FillCircle { center: Point, radius: f32, color: Color },
    /// Two-stop linear gradient fill of a (rounded) rect. `vertical` picks
    /// the axis; `radius` rounds corners (0 = square).
    FillGradient { rect: Rect, radius: f32, from: Color, to: Color, vertical: bool },
    /// A ring segment: stroke of thickness `thickness` along the circle of
    /// `radius` centered at `center`, from `start_deg` sweeping `sweep_deg`
    /// clockwise (0° = 3 o'clock). Powers progress rings and spinners.
    FillArc { center: Point, radius: f32, thickness: f32, start_deg: f32, sweep_deg: f32, color: Color },
    DrawText   { text: String, origin: Point, color: Color, px: f32, weight: FontWeight },
    /// Gaussian-approximate drop shadow. `radius` rounds the shadow's source
    /// shape to match rounded widgets — a square shadow behind a rounded fill
    /// leaks dark corner triangles.
    DrawShadow { rect: Rect, radius: f32, color: Color, blur: f32 },
    /// Raw pre-decoded RGBA pixel blit. `pixels` must be `width × height × 4` bytes.
    /// `opacity` (0.0-1.0) scales the blit's alpha — D108/Phase 26 Step 4's
    /// image load-in fade; `1.0` is the previous, fully-opaque behavior.
    BlitRgba   { pixels: Arc<Vec<u8>>, src_width: u32, src_height: u32, dest_rect: Rect, opacity: f32 },
    /// Push a clip rect — subsequent commands are confined to `rect` (intersected
    /// with any already-active clip). Must be paired with [`DrawCommand::PopClip`].
    PushClip   { rect: Rect },
    /// Restore the clip rect that was active before the matching [`DrawCommand::PushClip`].
    PopClip,
    /// Frosted-glass panel (D-DEF-012 backdrop blur): everything already
    /// drawn beneath `rect` is blurred and tinted behind a rounded panel.
    /// GPU-composited targets only; the CPU fallback renders a translucent
    /// tint (no blur) — honest degradation, not silence. `blur` is the
    /// Gaussian strength in logical px; `tint`'s alpha is the tint mix.
    BackdropBlur { rect: Rect, radius: f32, blur: f32, tint: Color },
    /// Fill `rect` with a registered GPU shader pipeline (D109/Phase 27).
    ///
    /// `pipeline_id` is the raw value of a `rosace-shader` `PipelineId`
    /// (this Layer-4 crate cannot import the Layer-5 typed id). `uniforms`
    /// are WGSL-uniform-layout bytes produced by `#[derive(ShaderUniforms)]`
    /// — opaque here. This command has NO CPU rasterization path by design:
    /// `SkiaCanvas::play_picture` collects it (see `take_shader_quads`) for
    /// the compositor to execute on the GPU at present time.
    ShaderFill { pipeline_id: u64, rect: Rect, uniforms: Vec<u8> },
}

impl DrawCommand {
    /// Return a copy of this command translated by (dx, dy) in logical pixels (D088).
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        fn shift(r: Rect, dx: f32, dy: f32) -> Rect {
            Rect {
                origin: Point { x: r.origin.x + dx, y: r.origin.y + dy },
                size: r.size,
            }
        }
        match self.clone() {
            Self::FillRect   { rect, color }           => Self::FillRect   { rect: shift(rect, dx, dy), color },
            Self::StrokeRect { rect, color, width }    => Self::StrokeRect { rect: shift(rect, dx, dy), color, width },
            Self::FillRRect  { rect, radius, color }   => Self::FillRRect  { rect: shift(rect, dx, dy), radius, color },
            Self::StrokeRRect { rect, radius, color, width } => Self::StrokeRRect { rect: shift(rect, dx, dy), radius, color, width },
            Self::FillCircle { center, radius, color } => Self::FillCircle { center: Point { x: center.x + dx, y: center.y + dy }, radius, color },
            Self::FillGradient { rect, radius, from, to, vertical } => Self::FillGradient { rect: shift(rect, dx, dy), radius, from, to, vertical },
            Self::FillArc { center, radius, thickness, start_deg, sweep_deg, color } => Self::FillArc { center: Point { x: center.x + dx, y: center.y + dy }, radius, thickness, start_deg, sweep_deg, color },
            Self::DrawText   { text, origin, color, px, weight } => Self::DrawText { text, origin: Point { x: origin.x + dx, y: origin.y + dy }, color, px, weight },
            Self::DrawShadow { rect, radius, color, blur } => Self::DrawShadow { rect: shift(rect, dx, dy), radius, color, blur },
            Self::BlitRgba   { pixels, src_width, src_height, dest_rect, opacity } =>
                Self::BlitRgba { pixels, src_width, src_height, dest_rect: shift(dest_rect, dx, dy), opacity },
            Self::PushClip   { rect }                  => Self::PushClip   { rect: shift(rect, dx, dy) },
            Self::PopClip                              => Self::PopClip,
            Self::ShaderFill { pipeline_id, rect, uniforms } =>
                Self::ShaderFill { pipeline_id, rect: shift(rect, dx, dy), uniforms },
            Self::BackdropBlur { rect, radius, blur, tint } =>
                Self::BackdropBlur { rect: shift(rect, dx, dy), radius, blur, tint },
        }
    }

    /// Return a copy of this command remapped from a `src`-rect-relative
    /// coordinate space to a `dst` one — translating AND scaling, unlike
    /// [`Self::offset`] which only translates. Backs Hero/shared-element
    /// transitions (D108/Phase 26 Step 5): a captured [`crate::Picture`]
    /// recorded at a widget's rect on one screen is replayed at a
    /// different-sized rect on the other screen's tag match, morphing
    /// between the two. Non-rect geometry (circle/arc radius, stroke/blur
    /// width, font size) has no independent x/y scale of its own, so it
    /// scales uniformly by the average of `sx`/`sy`.
    pub fn morph(&self, src_origin: Point, dst_origin: Point, sx: f32, sy: f32) -> Self {
        let s = (sx + sy) * 0.5;
        fn pt(p: Point, so: Point, do_: Point, sx: f32, sy: f32) -> Point {
            Point { x: do_.x + (p.x - so.x) * sx, y: do_.y + (p.y - so.y) * sy }
        }
        fn rc(r: Rect, so: Point, do_: Point, sx: f32, sy: f32) -> Rect {
            Rect {
                origin: pt(r.origin, so, do_, sx, sy),
                size: Size { width: r.size.width * sx, height: r.size.height * sy },
            }
        }
        match self.clone() {
            Self::FillRect   { rect, color }           => Self::FillRect   { rect: rc(rect, src_origin, dst_origin, sx, sy), color },
            Self::StrokeRect { rect, color, width }    => Self::StrokeRect { rect: rc(rect, src_origin, dst_origin, sx, sy), color, width: width * s },
            Self::FillRRect  { rect, radius, color }   => Self::FillRRect  { rect: rc(rect, src_origin, dst_origin, sx, sy), radius: radius * s, color },
            Self::StrokeRRect { rect, radius, color, width } => Self::StrokeRRect { rect: rc(rect, src_origin, dst_origin, sx, sy), radius: radius * s, color, width: width * s },
            Self::FillCircle { center, radius, color } => Self::FillCircle { center: pt(center, src_origin, dst_origin, sx, sy), radius: radius * s, color },
            Self::FillGradient { rect, radius, from, to, vertical } => Self::FillGradient { rect: rc(rect, src_origin, dst_origin, sx, sy), radius: radius * s, from, to, vertical },
            Self::FillArc { center, radius, thickness, start_deg, sweep_deg, color } => Self::FillArc { center: pt(center, src_origin, dst_origin, sx, sy), radius: radius * s, thickness: thickness * s, start_deg, sweep_deg, color },
            Self::DrawText   { text, origin, color, px, weight } => Self::DrawText { text, origin: pt(origin, src_origin, dst_origin, sx, sy), color, px: px * s, weight },
            Self::DrawShadow { rect, radius, color, blur } => Self::DrawShadow { rect: rc(rect, src_origin, dst_origin, sx, sy), radius: radius * s, color, blur: blur * s },
            Self::BlitRgba   { pixels, src_width, src_height, dest_rect, opacity } =>
                Self::BlitRgba { pixels, src_width, src_height, dest_rect: rc(dest_rect, src_origin, dst_origin, sx, sy), opacity },
            Self::PushClip   { rect }                  => Self::PushClip   { rect: rc(rect, src_origin, dst_origin, sx, sy) },
            Self::PopClip                              => Self::PopClip,
            // Uniform bytes are pipeline-private and cannot be remapped
            // generically — only the fill rect morphs. A shader whose
            // uniforms encode absolute positions won't track a Hero morph;
            // that's the shader author's contract, documented on ShaderFill.
            Self::ShaderFill { pipeline_id, rect, uniforms } =>
                Self::ShaderFill { pipeline_id, rect: rc(rect, src_origin, dst_origin, sx, sy), uniforms },
            Self::BackdropBlur { rect, radius, blur, tint } =>
                Self::BackdropBlur { rect: rc(rect, src_origin, dst_origin, sx, sy), radius: radius * s, blur: blur * s, tint },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn morph_maps_a_rect_from_its_source_origin_to_the_destination_origin_and_size() {
        // A widget captured at (0, 0, 100, 100) morphs to (200, 50, 40, 40) —
        // its own rect (same as src) must land exactly on dst.
        let cmd = DrawCommand::FillRect { rect: rect(0.0, 0.0, 100.0, 100.0), color: Color::RED };
        let morphed = cmd.morph(Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 50.0 }, 0.4, 0.4);
        match morphed {
            DrawCommand::FillRect { rect: r, .. } => {
                assert!((r.origin.x - 200.0).abs() < 0.001);
                assert!((r.origin.y - 50.0).abs() < 0.001);
                assert!((r.size.width - 40.0).abs() < 0.001, "100 * 0.4 = 40, got {}", r.size.width);
                assert!((r.size.height - 40.0).abs() < 0.001);
            }
            other => panic!("expected FillRect, got {other:?}"),
        }
    }

    #[test]
    fn morph_scales_geometry_nested_inside_the_captured_rect_proportionally() {
        // A circle centered at the MIDDLE of a 100x100 capture (50,50) must
        // land at the middle of the 40x40 destination (220,70), not at the
        // destination's origin.
        let cmd = DrawCommand::FillCircle { center: Point { x: 50.0, y: 50.0 }, radius: 10.0, color: Color::RED };
        let morphed = cmd.morph(Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 50.0 }, 0.4, 0.4);
        match morphed {
            DrawCommand::FillCircle { center, radius, .. } => {
                assert!((center.x - 220.0).abs() < 0.001, "expected 200 + 50*0.4 = 220, got {}", center.x);
                assert!((center.y - 70.0).abs() < 0.001, "expected 50 + 50*0.4 = 70, got {}", center.y);
                assert!((radius - 4.0).abs() < 0.001, "10 * 0.4 = 4, got {}", radius);
            }
            other => panic!("expected FillCircle, got {other:?}"),
        }
    }

    #[test]
    fn morph_at_identity_scale_and_matching_origins_is_a_no_op() {
        let cmd = DrawCommand::FillRect { rect: rect(10.0, 20.0, 30.0, 40.0), color: Color::RED };
        let morphed = cmd.morph(Point { x: 10.0, y: 20.0 }, Point { x: 10.0, y: 20.0 }, 1.0, 1.0);
        match morphed {
            DrawCommand::FillRect { rect: r, .. } => {
                assert_eq!(r.origin.x, 10.0);
                assert_eq!(r.origin.y, 20.0);
                assert_eq!(r.size.width, 30.0);
                assert_eq!(r.size.height, 40.0);
            }
            other => panic!("expected FillRect, got {other:?}"),
        }
    }
}
