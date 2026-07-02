use std::sync::Arc;

use tezzera_core::types::{Point, Rect};

use crate::canvas::Color;

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
    DrawText   { text: String, origin: Point, color: Color, px: f32 },
    /// Gaussian-approximate drop shadow. `radius` rounds the shadow's source
    /// shape to match rounded widgets — a square shadow behind a rounded fill
    /// leaks dark corner triangles.
    DrawShadow { rect: Rect, radius: f32, color: Color, blur: f32 },
    /// Raw pre-decoded RGBA pixel blit. `pixels` must be `width × height × 4` bytes.
    BlitRgba   { pixels: Arc<Vec<u8>>, src_width: u32, src_height: u32, dest_rect: Rect },
    /// Push a clip rect — subsequent commands are confined to `rect` (intersected
    /// with any already-active clip). Must be paired with [`DrawCommand::PopClip`].
    PushClip   { rect: Rect },
    /// Restore the clip rect that was active before the matching [`DrawCommand::PushClip`].
    PopClip,
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
            Self::DrawText   { text, origin, color, px } => Self::DrawText { text, origin: Point { x: origin.x + dx, y: origin.y + dy }, color, px },
            Self::DrawShadow { rect, radius, color, blur } => Self::DrawShadow { rect: shift(rect, dx, dy), radius, color, blur },
            Self::BlitRgba   { pixels, src_width, src_height, dest_rect } =>
                Self::BlitRgba { pixels, src_width, src_height, dest_rect: shift(dest_rect, dx, dy) },
            Self::PushClip   { rect }                  => Self::PushClip   { rect: shift(rect, dx, dy) },
            Self::PopClip                              => Self::PopClip,
        }
    }
}
