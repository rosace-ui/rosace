use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use tezzera_core::types::{Point, Rect, Size};

/// TEZZERA's 2D drawing canvas backed by tiny-skia.
///
/// Replaces the placeholder `Canvas` in `tezzera-core` for the Phase 1 desktop
/// target. All drawing operations are performed on a CPU pixel buffer; no native
/// graphics library is required.
pub struct SkiaCanvas {
    pixmap: Pixmap,
    /// Device pixel ratio (e.g. 2.0 on Retina). All draw coordinates are in
    /// logical pixels; `play_picture` multiplies them by this before writing
    /// physical pixels, so the full HiDPI buffer is used without blurry upscaling.
    scale: f32,
    /// True after any draw call (other than `clear_transparent`). Used by the
    /// platform to skip the overlay Porter-Duff blend when nothing was drawn.
    has_drawn: bool,
    /// Active clip rect in PHYSICAL pixel coordinates, stored as (x, y, right, bottom)
    /// right-exclusive. `None` means no clipping. Managed by `play_picture`.
    clip: Option<(i32, i32, i32, i32)>,
}

/// An RGBA color value.
#[derive(Debug, Clone, Copy)]
pub struct Color {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
    /// Alpha channel (0–255).
    pub a: u8,
}

impl Color {
    /// Create an opaque color from red, green, and blue components.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a color with explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque red.
    pub const RED: Color = Color::rgb(255, 0, 0);
    /// Opaque green.
    pub const GREEN: Color = Color::rgb(0, 255, 0);
    /// Opaque blue.
    pub const BLUE: Color = Color::rgb(0, 0, 255);
    /// Fully transparent.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
}

// ── Clip helpers (physical pixel space) ──────────────────────────────────────

/// Intersect a rect (x, y, w, h) with a clip region (cx, cy, cr, cb).
/// Returns the clipped (x, y, w, h) or None if fully outside.
#[inline]
fn clip_xywh(
    x: f32, y: f32, w: f32, h: f32,
    clip: (i32, i32, i32, i32),
) -> Option<(f32, f32, f32, f32)> {
    let (cx, cy, cr, cb) = clip;
    let x0 = x.max(cx as f32);
    let y0 = y.max(cy as f32);
    let x1 = (x + w).min(cr as f32);
    let y1 = (y + h).min(cb as f32);
    if x1 > x0 && y1 > y0 { Some((x0, y0, x1 - x0, y1 - y0)) } else { None }
}

/// True if a rect overlaps the clip region (used for early cull on circles/rrects).
#[inline]
fn overlaps_clip(x: f32, y: f32, w: f32, h: f32, clip: (i32, i32, i32, i32)) -> bool {
    let (cx, cy, cr, cb) = clip;
    x + w > cx as f32 && y + h > cy as f32 && x < cr as f32 && y < cb as f32
}

impl SkiaCanvas {
    /// Create a canvas at physical pixel size with a device pixel ratio of 1.0.
    pub fn new(width: u32, height: u32) -> Self {
        Self::new_hidpi(width, height, 1.0)
    }

    /// Create a canvas for a HiDPI display.
    ///
    /// `phys_width` / `phys_height` are the framebuffer dimensions in physical
    /// pixels. `scale` is the device pixel ratio (e.g. 2.0 on Retina).
    /// All draw coordinates passed via [`play_picture`] are in logical pixels
    /// and are multiplied by `scale` before writing to the pixmap.
    pub fn new_hidpi(phys_width: u32, phys_height: u32, scale: f32) -> Self {
        Self {
            pixmap: Pixmap::new(phys_width, phys_height).expect("failed to create pixmap"),
            scale: scale.max(1.0),
            has_drawn: false,
            clip: None,
        }
    }

    /// Physical pixel width of the underlying framebuffer.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Physical pixel height of the underlying framebuffer.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Logical width (physical / scale). Use this for layout calculations.
    pub fn logical_width(&self) -> u32 {
        (self.pixmap.width() as f32 / self.scale).round() as u32
    }

    /// Logical height (physical / scale). Use this for layout calculations.
    pub fn logical_height(&self) -> u32 {
        (self.pixmap.height() as f32 / self.scale).round() as u32
    }

    /// Device pixel ratio for this canvas.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// True if any draw operation (other than `clear_transparent`) has been called.
    ///
    /// Used by the platform to skip the overlay Porter-Duff blend when the
    /// overlay canvas has no content, avoiding O(pixels) work every frame.
    pub fn has_drawn(&self) -> bool {
        self.has_drawn
    }

    /// Fill the entire canvas with a solid color.
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(
            tiny_skia::Color::from_rgba8(color.r, color.g, color.b, color.a),
        );
        self.has_drawn = true;
    }

    /// Fill the entire canvas with fully-transparent pixels (D078).
    ///
    /// Resets `has_drawn` so the platform can skip the overlay blend this frame.
    pub fn clear_transparent(&mut self) {
        self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
        self.has_drawn = false;
    }

    /// Fill a rectangle with a solid color.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if color.a == 0 { return; }
        let (mut x, mut y, mut w, mut h) = (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
        if w < 0.5 || h < 0.5 { return; }

        if let Some(clip) = self.clip {
            match clip_xywh(x, y, w, h, clip) {
                Some((cx, cy, cw, ch)) => { x = cx; y = cy; w = cw; h = ch; }
                None => return,
            }
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = false;
        if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            self.pixmap.fill_rect(r, &paint, Transform::identity(), None);
        }
        self.has_drawn = true;
    }

    /// Draw a rectangle outline with the given stroke width.
    pub fn stroke_rect(&mut self, rect: Rect, color: Color, stroke_width: f32) {
        // Quick cull against clip before paying tiny_skia path overhead.
        if let Some(clip) = self.clip {
            if !overlaps_clip(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, clip) {
                return;
            }
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        let Some(skia_rect) = tiny_skia::Rect::from_xywh(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        ) else {
            return;
        };
        let path = PathBuilder::from_rect(skia_rect);
        let stroke = tiny_skia::Stroke {
            width: stroke_width,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        self.has_drawn = true;
    }

    /// Draw a filled circle centered at `center` with the given `radius`.
    pub fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        if color.a == 0 || radius < 0.5 { return; }
        if let Some(clip) = self.clip {
            if !overlaps_clip(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0, clip) {
                return;
            }
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        let mut pb = PathBuilder::new();
        pb.push_circle(center.x, center.y, radius);
        if let Some(path) = pb.finish() {
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        self.has_drawn = true;
    }

    /// Draw a text placeholder at `origin`.
    pub fn draw_text_placeholder(&mut self, text: &str, origin: Point, color: Color) {
        let width = text.len() as f32 * 8.0;
        let height = 16.0;
        self.fill_rect(
            Rect {
                origin,
                size: Size { width, height },
            },
            color,
        );
    }

    /// Draw real text glyphs at `origin` using `font` at `px` size.
    ///
    /// `origin` is the top-left of the glyph bounding box. Each character is
    /// rasterized and alpha-blended directly into the pixel buffer — no per-pixel
    /// fill_rect overhead.
    pub fn draw_text(&mut self, text: &str, origin: Point, color: Color, font: &crate::font::FontCache, px: f32) {
        if color.a == 0 || text.is_empty() { return; }

        let canvas_w = self.pixmap.width() as i32;
        let canvas_h = self.pixmap.height() as i32;
        let ascender = font.ascender(px);

        // Resolve clip bounds in pixel coordinates once.
        let (clip_x0, clip_y0, clip_x1, clip_y1) = match self.clip {
            Some((cx, cy, cr, cb)) => (cx, cy, cr, cb),
            None                   => (0, 0, canvas_w, canvas_h),
        };

        let mut cursor_x = origin.x;

        // Obtain a mutable slice of the pixel buffer. Because `font` is a
        // separate argument (not a field of SkiaCanvas), holding `dst` and
        // calling `font.rasterize` in the loop has no borrow conflict.
        let dst = self.pixmap.data_mut();

        for ch in text.chars() {
            let (metrics, bitmap) = font.rasterize(ch, px);

            if metrics.width == 0 || metrics.height == 0 {
                cursor_x += metrics.advance_width;
                continue;
            }

            for row in 0..metrics.height {
                // Place glyph relative to the shared baseline.
                let py = origin.y as i32 + ascender
                    - metrics.ymin
                    - metrics.height as i32
                    + row as i32;
                if py < clip_y0 || py >= clip_y1 { continue; }

                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 { continue; }

                    let px_xi = cursor_x as i32 + col as i32 + metrics.xmin;
                    if px_xi < clip_x0 || px_xi >= clip_x1 { continue; }

                    // Premultiplied source-over blend into tiny_skia's premul buffer.
                    let src_a = (coverage as u32 * color.a as u32) / 255;
                    let src_r = color.r as u32 * src_a / 255;
                    let src_g = color.g as u32 * src_a / 255;
                    let src_b = color.b as u32 * src_a / 255;
                    let inv   = 255 - src_a;

                    let di = (py * canvas_w + px_xi) as usize * 4;
                    if di + 3 >= dst.len() { continue; }
                    dst[di]     = (src_r + dst[di]     as u32 * inv / 255) as u8;
                    dst[di + 1] = (src_g + dst[di + 1] as u32 * inv / 255) as u8;
                    dst[di + 2] = (src_b + dst[di + 2] as u32 * inv / 255) as u8;
                    dst[di + 3] = (src_a + dst[di + 3] as u32 * inv / 255) as u8;
                }
            }

            cursor_x += metrics.advance_width;
        }
        self.has_drawn = true;
    }

    /// Fill a rounded rectangle using three overlapping rects and four corner circles.
    pub fn fill_rrect(&mut self, rect: Rect, radius: f32, color: Color) {
        if color.a == 0 { return; }
        // Early cull — skip if completely outside clip.
        if let Some(clip) = self.clip {
            if !overlaps_clip(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, clip) {
                return;
            }
        }
        let r = radius.min(rect.size.width / 2.0).min(rect.size.height / 2.0);
        if r < 0.5 {
            self.fill_rect(rect, color);
            return;
        }
        let x = rect.origin.x;
        let y = rect.origin.y;
        let w = rect.size.width;
        let h = rect.size.height;
        self.fill_rect(Rect { origin: Point { x: x + r, y }, size: Size { width: w - r * 2.0, height: h } }, color);
        self.fill_rect(Rect { origin: Point { x, y: y + r }, size: Size { width: w, height: h - r * 2.0 } }, color);
        self.fill_circle(Point { x: x + r,     y: y + r     }, r, color);
        self.fill_circle(Point { x: x + w - r, y: y + r     }, r, color);
        self.fill_circle(Point { x: x + r,     y: y + h - r }, r, color);
        self.fill_circle(Point { x: x + w - r, y: y + h - r }, r, color);
    }

    /// Replay a [`Picture`] (display list) onto this canvas.
    ///
    /// All draw-command coordinates are in **logical pixels**. They are
    /// multiplied by `self.scale` before writing to the physical pixmap, so
    /// the full HiDPI framebuffer resolution is used and there is no
    /// nearest-neighbour upscaling blur.
    ///
    /// `PushClip` / `PopClip` commands maintain a clip stack so that
    /// `ScrollView` children are confined to their viewport.
    pub fn play_picture(&mut self, picture: &crate::picture::Picture, font: &crate::font::FontCache) {
        use crate::draw_command::DrawCommand;
        let s = self.scale;
        let sr = |r: Rect| Rect {
            origin: Point { x: r.origin.x * s, y: r.origin.y * s },
            size:   Size  { width: r.size.width * s, height: r.size.height * s },
        };
        let sp = |p: Point| Point { x: p.x * s, y: p.y * s };

        // Clip stack — each entry is the clip that was active BEFORE the matching PushClip.
        let mut clip_stack: Vec<Option<(i32, i32, i32, i32)>> = Vec::new();
        // Save and restore the outer clip (normally None at the top level).
        let outer_clip = self.clip;

        for cmd in &picture.commands {
            match cmd {
                DrawCommand::PushClip { rect } => {
                    let r = sr(*rect);
                    let x0 = r.origin.x as i32;
                    let y0 = r.origin.y as i32;
                    let x1 = (r.origin.x + r.size.width) as i32;
                    let y1 = (r.origin.y + r.size.height) as i32;
                    let new_clip = if let Some((cx, cy, cr, cb)) = self.clip {
                        // Intersect with the already-active clip.
                        let ix0 = x0.max(cx);
                        let iy0 = y0.max(cy);
                        let ix1 = x1.min(cr);
                        let iy1 = y1.min(cb);
                        if ix1 > ix0 && iy1 > iy0 { Some((ix0, iy0, ix1, iy1)) } else { None }
                    } else {
                        if x1 > x0 && y1 > y0 { Some((x0, y0, x1, y1)) } else { None }
                    };
                    clip_stack.push(self.clip);
                    self.clip = new_clip;
                }

                DrawCommand::PopClip => {
                    // pop() returns Option<Option<...>>; unwrap_or restores None on underflow.
                    self.clip = clip_stack.pop().unwrap_or(None);
                }

                DrawCommand::FillRect { rect, color } => self.fill_rect(sr(*rect), *color),
                DrawCommand::StrokeRect { rect, color, width } => self.stroke_rect(sr(*rect), *color, *width * s),
                DrawCommand::FillRRect { rect, radius, color } => self.fill_rrect(sr(*rect), *radius * s, *color),
                DrawCommand::FillCircle { center, radius, color } => self.fill_circle(sp(*center), *radius * s, *color),
                DrawCommand::DrawText { text, origin, color, px } => {
                    self.draw_text(text, sp(*origin), *color, font, *px * s);
                }
                DrawCommand::DrawShadow { rect, color, blur } => {
                    let steps = (*blur as u32).min(8).max(1);
                    for i in 0..steps {
                        let alpha = (color.a as f32 * (1.0 - i as f32 / steps as f32) / steps as f32) as u8;
                        let spread = i as f32 * *blur / steps as f32 * s;
                        let scaled = sr(*rect);
                        let shifted = Rect {
                            origin: Point { x: scaled.origin.x + spread, y: scaled.origin.y + spread },
                            size: scaled.size,
                        };
                        self.fill_rect(shifted, Color::rgba(color.r, color.g, color.b, alpha));
                    }
                }
                DrawCommand::BlitRgba { pixels, src_width, src_height, dest_rect } => {
                    self.blit_rgba(pixels, *src_width, *src_height, sr(*dest_rect));
                }
            }
        }

        // Restore clip to what it was before play_picture (handles nested calls).
        self.clip = outer_clip;
    }

    /// Blit pre-decoded RGBA pixel data into `dest_rect`.
    ///
    /// `pixels` must be `src_width × src_height × 4` bytes (RGBA). The source
    /// is scaled to fill `dest_rect` using nearest-neighbour sampling. Pixels
    /// outside the canvas bounds (and current clip) are skipped.
    pub fn blit_rgba(&mut self, pixels: &[u8], src_w: u32, src_h: u32, dest: Rect) {
        let cw = self.pixmap.width() as i32;
        let ch = self.pixmap.height() as i32;

        let dx = dest.origin.x as i32;
        let dy = dest.origin.y as i32;
        let dw = dest.size.width as i32;
        let dh = dest.size.height as i32;

        // Merge canvas bounds with active clip into a single test region.
        let (cx0, cy0, cx1, cy1) = match self.clip {
            Some((cx, cy, cr, cb)) => (cx.max(0), cy.max(0), cr.min(cw), cb.min(ch)),
            None                   => (0, 0, cw, ch),
        };

        let dst = self.pixmap.data_mut();

        for row in 0..dh {
            let src_row = (row * src_h as i32 / dw.max(1)) as u32;
            let py = dy + row;
            if py < cy0 || py >= cy1 { continue; }
            for col in 0..dw {
                let src_col = (col * src_w as i32 / dw.max(1)) as u32;
                let px = dx + col;
                if px < cx0 || px >= cx1 { continue; }
                let si = (src_row * src_w + src_col) as usize * 4;
                let di = (py * cw + px) as usize * 4;
                if si + 3 >= pixels.len() || di + 3 >= dst.len() { continue; }
                let alpha = pixels[si + 3] as u32;
                let inv = 255 - alpha;
                dst[di]     = ((pixels[si]     as u32 * alpha + dst[di]     as u32 * inv) / 255) as u8;
                dst[di + 1] = ((pixels[si + 1] as u32 * alpha + dst[di + 1] as u32 * inv) / 255) as u8;
                dst[di + 2] = ((pixels[si + 2] as u32 * alpha + dst[di + 2] as u32 * inv) / 255) as u8;
                dst[di + 3] = 255;
            }
        }
        self.has_drawn = true;
    }

    /// Returns the raw RGBA pixel data as a byte slice.
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Returns the raw RGBA pixel data as a mutable byte slice.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        self.pixmap.data_mut()
    }

    /// Encode the canvas contents as a PNG byte vector, returning `None` on error.
    pub fn encode_png(&self) -> Option<Vec<u8>> {
        self.pixmap.encode_png().ok()
    }
}
