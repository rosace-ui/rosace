use std::collections::HashMap;

use tiny_skia::{FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap, Shader, SpreadMode, Stroke, Transform};
use tezzera_core::types::{Point, Rect, Size};

/// Cubic Bézier circle-approximation constant (4/3 · tan(π/8)).
const KAPPA: f32 = 0.552_285;

/// Perceptual coverage curve for text anti-aliasing. Linear alpha makes
/// dark-on-light stems look anemic (mid coverages read too light); a mild
/// gamma boost on the coverage ramp keeps edges smooth while restoring
/// stem weight. One-time 256-entry table.
fn text_gamma(cov: u32) -> u32 {
    use std::sync::OnceLock;
    static LUT: OnceLock<[u8; 256]> = OnceLock::new();
    let lut = LUT.get_or_init(|| {
        let mut t = [0u8; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((i as f32 / 255.0).powf(1.0 / 1.22) * 255.0).round() as u8;
        }
        t
    });
    lut[cov as usize] as u32
}

/// Exact-rounding division by 255 without an integer divide.
#[inline(always)]
fn d255(x: u32) -> u32 {
    let t = x + 128;
    (t + (t >> 8)) >> 8
}

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
    /// True when this canvas's pixels changed since the last present. The frame
    /// loop sets it whenever it repaints; the platform consumes it via
    /// [`take_frame_dirty`] to skip the GPU texture upload on clean frames
    /// (D089). Starts `true` so the first frame always uploads.
    frame_dirty: bool,
    /// Active clip rect in PHYSICAL pixel coordinates, stored as (x, y, right, bottom)
    /// right-exclusive. `None` means no clipping. Managed by `play_picture`.
    clip: Option<(i32, i32, i32, i32)>,
    /// Rasterized clip masks for path fills (circles, rounded rects), keyed by
    /// the clip tuple. Built lazily on first path fill under a given clip and
    /// reused for the lifetime of the canvas (viewport clips are stable).
    clip_masks: HashMap<(i32, i32, i32, i32), Mask>,
    /// Blurred shadow masks keyed by (width, height, blur, corner radius) in
    /// physical pixels. Blurred once per unique geometry, replayed as a blit.
    shadow_cache: HashMap<(u32, u32, u32, u32), ShadowMask>,
}

/// A pre-blurred shadow coverage mask (single channel).
struct ShadowMask {
    w: usize,
    h: usize,
    /// Blur margin in pixels on each side of the nominal rect.
    margin: i32,
    data: Vec<u8>,
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

/// Build (or fetch) the rasterized mask for `clip`, storing it in `masks`.
///
/// Free function (not a method) so callers can hold a `&Mask` from `masks`
/// while mutably borrowing `pixmap` — disjoint field borrows.
fn ensure_clip_mask(
    masks: &mut HashMap<(i32, i32, i32, i32), Mask>,
    clip: (i32, i32, i32, i32),
    width: u32,
    height: u32,
) {
    if masks.contains_key(&clip) {
        return;
    }
    let Some(mut mask) = Mask::new(width, height) else { return };
    let (x0, y0, x1, y1) = clip;
    let mut pb = PathBuilder::new();
    if let Some(r) = tiny_skia::Rect::from_ltrb(x0 as f32, y0 as f32, x1 as f32, y1 as f32) {
        pb.push_rect(r);
    }
    if let Some(path) = pb.finish() {
        mask.fill_path(&path, FillRule::Winding, false, Transform::identity());
        masks.insert(clip, mask);
    }
}

/// Build a rounded-rect path with proper cubic Bézier corner arcs.
fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let k = KAPPA * r;
    let (x1, y1) = (x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// One horizontal sliding-window box-blur pass with clamp-to-edge sampling.
fn box_blur_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let norm = (2 * r + 1) as u32;
    for y in 0..h {
        let row = y * w;
        let mut acc: u32 = src[row] as u32 * r as u32;
        for i in 0..=r {
            acc += src[row + i.min(w - 1)] as u32;
        }
        for x in 0..w {
            dst[row + x] = (acc / norm) as u8;
            let add = src[row + (x + r + 1).min(w - 1)] as u32;
            let sub = src[row + x.saturating_sub(r)] as u32;
            acc = acc + add - sub;
        }
    }
}

/// One vertical sliding-window box-blur pass with clamp-to-edge sampling.
fn box_blur_v(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let norm = (2 * r + 1) as u32;
    for x in 0..w {
        let mut acc: u32 = src[x] as u32 * r as u32;
        for i in 0..=r {
            acc += src[i.min(h - 1) * w + x] as u32;
        }
        for y in 0..h {
            dst[y * w + x] = (acc / norm) as u8;
            let add = src[(y + r + 1).min(h - 1) * w + x] as u32;
            let sub = src[y.saturating_sub(r) * w + x] as u32;
            acc = acc + add - sub;
        }
    }
}

/// Rasterize and blur a shadow coverage mask for a `w`×`h` rounded rect
/// (corner `radius` px) at `blur` px.
///
/// The source shape matches the widget's rounded geometry so the blurred
/// shadow hugs the corners instead of leaking square corner triangles.
/// Three box-blur passes per axis approximate a Gaussian (σ ≈ blur/2).
fn build_shadow_mask(w: u32, h: u32, blur: u32, radius: u32) -> ShadowMask {
    let margin = (2 * blur) as i32 + 1;
    let mw = w as usize + 2 * margin as usize;
    let mh = h as usize + 2 * margin as usize;
    let mut data = vec![0u8; mw * mh];
    for row in margin as usize..margin as usize + h as usize {
        let s = row * mw + margin as usize;
        data[s..s + w as usize].fill(255);
    }

    // Carve the corners with distance-based coverage. Exactness is not
    // critical — the blur softens the edge — but the corner mass must go.
    let r = (radius as f32).min(w as f32 / 2.0).min(h as f32 / 2.0);
    if r >= 1.0 {
        let m = margin as f32;
        let centers = [
            (m + r,             m + r),
            (m + w as f32 - r,  m + r),
            (m + r,             m + h as f32 - r),
            (m + w as f32 - r,  m + h as f32 - r),
        ];
        let corners = [
            (m,                m,                m + r,            m + r),
            (m + w as f32 - r, m,                m + w as f32,     m + r),
            (m,                m + h as f32 - r, m + r,            m + h as f32),
            (m + w as f32 - r, m + h as f32 - r, m + w as f32,     m + h as f32),
        ];
        for (i, &(x0, y0, x1, y1)) in corners.iter().enumerate() {
            let (cx, cy) = centers[i];
            for py in y0 as usize..(y1.ceil() as usize).min(mh) {
                for px in x0 as usize..(x1.ceil() as usize).min(mw) {
                    let dx = px as f32 + 0.5 - cx;
                    let dy = py as f32 + 0.5 - cy;
                    let d = (dx * dx + dy * dy).sqrt();
                    let coverage = (r + 0.5 - d).clamp(0.0, 1.0);
                    data[py * mw + px] = (coverage * 255.0) as u8;
                }
            }
        }
    }

    let br = (blur as usize / 2).max(1);
    let mut tmp = vec![0u8; mw * mh];
    for _ in 0..3 {
        box_blur_h(&data, &mut tmp, mw, mh, br);
        box_blur_v(&tmp, &mut data, mw, mh, br);
    }
    ShadowMask { w: mw, h: mh, margin, data }
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
            frame_dirty: true,
            clip: None,
            clip_masks: HashMap::new(),
            shadow_cache: HashMap::new(),
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

    /// Mark this canvas's pixels as changed this frame (D089). The frame loop
    /// calls this whenever it repaints the canvas, so the platform re-uploads
    /// its GPU texture; clean frames leave the flag false and skip the upload.
    pub fn mark_frame_dirty(&mut self) {
        self.frame_dirty = true;
    }

    /// Return whether the canvas changed since the last present and reset the
    /// flag to false (D089). Called once per frame by the platform present.
    pub fn take_frame_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.frame_dirty, false)
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
    ///
    /// Edges are snapped to the physical pixel grid: adjacent widgets that
    /// share a computed edge land on the same pixel column/row, so there are
    /// no hairline seams and no sub-pixel shimmer during layout changes.
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

        // Snap edges (not origin+size) so both sides of a shared boundary
        // round identically. Guarantee at least 1px after snapping.
        let x0 = x.round();
        let y0 = y.round();
        let x1 = (x + w).round().max(x0 + 1.0);
        let y1 = (y + h).round().max(y0 + 1.0);

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = false;
        if let Some(r) = tiny_skia::Rect::from_ltrb(x0, y0, x1, y1) {
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
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
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
        let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
        self.has_drawn = true;
    }

    /// Draw a filled circle centered at `center` with the given `radius`.
    pub fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        if color.a == 0 || radius < 0.5 { return; }
        if let Some(clip) = self.clip {
            if !overlaps_clip(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0, clip) {
                return;
            }
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        let mut pb = PathBuilder::new();
        pb.push_circle(center.x, center.y, radius);
        if let Some(path) = pb.finish() {
            let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask,
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
    /// `origin` is the top-left of the glyph bounding box. Glyph x positions
    /// are rounded (not truncated) and kerning pairs are applied, matching
    /// [`FontCache::measure_text`]. Blending uses an exact divide-free
    /// source-over with a straight-store fast path for opaque pixels.
    pub fn draw_text(&mut self, text: &str, origin: Point, color: Color, font: &crate::font::FontCache, px: f32) {
        self.draw_text_weighted(text, origin, color, font, px, crate::font::FontWeight::Regular);
    }

    /// Weighted variant: routes each character through the bold face and the
    /// Unicode fallback chain, applies kerning within a face, and blends
    /// with the perceptual coverage curve.
    pub fn draw_text_weighted(&mut self, text: &str, origin: Point, color: Color, font: &crate::font::FontCache, px: f32, weight: crate::font::FontWeight) {
        if color.a == 0 || text.is_empty() { return; }

        let canvas_w = self.pixmap.width() as i32;
        let canvas_h = self.pixmap.height() as i32;
        let ascender = font.ascender(px);

        // Resolve clip bounds clamped to the canvas so the inner loop needs
        // no per-pixel buffer-length check.
        let (clip_x0, clip_y0, clip_x1, clip_y1) = match self.clip {
            Some((cx, cy, cr, cb)) => (cx.max(0), cy.max(0), cr.min(canvas_w), cb.min(canvas_h)),
            None                   => (0, 0, canvas_w, canvas_h),
        };
        if clip_x1 <= clip_x0 || clip_y1 <= clip_y0 { return; }

        let base_y = origin.y.round() as i32 + ascender;
        let mut cursor_x = origin.x;
        let mut prev: Option<char> = None;
        let color_a = color.a as u32;

        // Obtain a mutable slice of the pixel buffer. Because `font` is a
        // separate argument (not a field of SkiaCanvas), holding `dst` and
        // calling `font.glyph` in the loop has no borrow conflict.
        let dst = self.pixmap.data_mut();

        for ch in text.chars() {
            if let Some(p) = prev {
                cursor_x += font.kern_weighted(p, ch, px, weight);
            }
            prev = Some(ch);

            let glyph = font.glyph_weighted(ch, px, weight);
            let (metrics, bitmap) = (&glyph.0, &glyph.1);

            if metrics.width == 0 || metrics.height == 0 {
                cursor_x += metrics.advance_width;
                continue;
            }

            let gx = cursor_x.round() as i32 + metrics.xmin;
            let gy = base_y - metrics.ymin - metrics.height as i32;

            for row in 0..metrics.height {
                let py = gy + row as i32;
                if py < clip_y0 || py >= clip_y1 { continue; }
                let row_base = (py * canvas_w) as usize * 4;
                let src_row = row * metrics.width;

                for col in 0..metrics.width {
                    let coverage = text_gamma(bitmap[src_row + col] as u32);
                    if coverage == 0 { continue; }

                    let px_xi = gx + col as i32;
                    if px_xi < clip_x0 || px_xi >= clip_x1 { continue; }

                    let di = row_base + px_xi as usize * 4;
                    if coverage == 255 && color_a == 255 {
                        // Fully-covered opaque pixel — straight store.
                        dst[di]     = color.r;
                        dst[di + 1] = color.g;
                        dst[di + 2] = color.b;
                        dst[di + 3] = 255;
                    } else {
                        // Premultiplied source-over blend into the premul buffer.
                        let src_a = d255(coverage * color_a);
                        let inv   = 255 - src_a;
                        dst[di]     = (d255(color.r as u32 * src_a) + d255(dst[di]     as u32 * inv)) as u8;
                        dst[di + 1] = (d255(color.g as u32 * src_a) + d255(dst[di + 1] as u32 * inv)) as u8;
                        dst[di + 2] = (d255(color.b as u32 * src_a) + d255(dst[di + 2] as u32 * inv)) as u8;
                        dst[di + 3] = (src_a + d255(dst[di + 3] as u32 * inv)) as u8;
                    }
                }
            }

            cursor_x += metrics.advance_width;
        }
        self.has_drawn = true;
    }

    /// Fill a rounded rectangle as a single anti-aliased path.
    ///
    /// One path fill — no seams between corner and edge geometry, and
    /// translucent colors blend exactly once per pixel.
    pub fn fill_rrect(&mut self, rect: Rect, radius: f32, color: Color) {
        if color.a == 0 { return; }
        if let Some(clip) = self.clip {
            if !overlaps_clip(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, clip) {
                return;
            }
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
        }
        let r = radius.min(rect.size.width / 2.0).min(rect.size.height / 2.0);
        if r < 0.5 {
            self.fill_rect(rect, color);
            return;
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        if let Some(path) = rounded_rect_path(
            rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, r,
        ) {
            let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
        self.has_drawn = true;
    }

    /// Stroke a rounded-rectangle outline along the same path geometry as
    /// [`SkiaCanvas::fill_rrect`], so borders hug rounded fills exactly.
    pub fn stroke_rrect(&mut self, rect: Rect, radius: f32, color: Color, stroke_width: f32) {
        if color.a == 0 { return; }
        if let Some(clip) = self.clip {
            if !overlaps_clip(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, clip) {
                return;
            }
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
        }
        let r = radius.min(rect.size.width / 2.0).min(rect.size.height / 2.0);
        if r < 0.5 {
            self.stroke_rect(rect, color, stroke_width);
            return;
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        if let Some(path) = rounded_rect_path(
            rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, r,
        ) {
            let stroke = tiny_skia::Stroke { width: stroke_width, ..Default::default() };
            let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
            self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
        }
        self.has_drawn = true;
    }

    /// Draw a soft drop shadow for a rounded rect with a Gaussian-approximate
    /// blur. `radius` must match the widget's corner radius so the shadow hugs
    /// the rounded shape.
    ///
    /// The blurred coverage mask is computed once per unique
    /// (width, height, blur, radius) and cached; draws are a tinted blit.
    pub fn draw_shadow(&mut self, rect: Rect, radius: f32, color: Color, blur: f32) {
        if color.a == 0 { return; }
        let blur = blur.max(0.0);
        if blur < 0.5 {
            self.fill_rrect(rect, radius, color);
            return;
        }
        let w = rect.size.width.round().max(1.0) as u32;
        let h = rect.size.height.round().max(1.0) as u32;
        let b = blur.round() as u32;
        let rad = radius.max(0.0).round() as u32;
        let key = (w, h, b, rad);
        self.shadow_cache
            .entry(key)
            .or_insert_with(|| build_shadow_mask(w, h, b, rad));

        let canvas_w = self.pixmap.width() as i32;
        let canvas_h = self.pixmap.height() as i32;
        let (clip_x0, clip_y0, clip_x1, clip_y1) = match self.clip {
            Some((cx, cy, cr, cb)) => (cx.max(0), cy.max(0), cr.min(canvas_w), cb.min(canvas_h)),
            None                   => (0, 0, canvas_w, canvas_h),
        };
        if clip_x1 <= clip_x0 || clip_y1 <= clip_y0 { return; }

        let mask = &self.shadow_cache[&key];
        let ox = rect.origin.x.round() as i32 - mask.margin;
        let oy = rect.origin.y.round() as i32 - mask.margin;
        let color_a = color.a as u32;
        let dst = self.pixmap.data_mut();

        for row in 0..mask.h {
            let py = oy + row as i32;
            if py < clip_y0 || py >= clip_y1 { continue; }
            let row_base = (py * canvas_w) as usize * 4;
            let src_row = row * mask.w;

            for col in 0..mask.w {
                let coverage = mask.data[src_row + col] as u32;
                if coverage == 0 { continue; }

                let px_xi = ox + col as i32;
                if px_xi < clip_x0 || px_xi >= clip_x1 { continue; }

                let src_a = d255(coverage * color_a);
                if src_a == 0 { continue; }
                let inv = 255 - src_a;
                let di = row_base + px_xi as usize * 4;
                dst[di]     = (d255(color.r as u32 * src_a) + d255(dst[di]     as u32 * inv)) as u8;
                dst[di + 1] = (d255(color.g as u32 * src_a) + d255(dst[di + 1] as u32 * inv)) as u8;
                dst[di + 2] = (d255(color.b as u32 * src_a) + d255(dst[di + 2] as u32 * inv)) as u8;
                dst[di + 3] = (src_a + d255(dst[di + 3] as u32 * inv)) as u8;
            }
        }
        self.has_drawn = true;
    }

    /// Fill a (rounded) rect with a two-stop linear gradient.
    pub fn fill_gradient(&mut self, rect: Rect, radius: f32, from: Color, to: Color, vertical: bool) {
        if let Some(clip) = self.clip {
            if !overlaps_clip(rect.origin.x, rect.origin.y, rect.size.width, rect.size.height, clip) { return; }
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
        }
        let (x, y, w, h) = (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
        let (p0, p1) = if vertical {
            (tiny_skia::Point::from_xy(x, y), tiny_skia::Point::from_xy(x, y + h))
        } else {
            (tiny_skia::Point::from_xy(x, y), tiny_skia::Point::from_xy(x + w, y))
        };
        let stops = vec![
            GradientStop::new(0.0, tiny_skia::Color::from_rgba8(from.r, from.g, from.b, from.a)),
            GradientStop::new(1.0, tiny_skia::Color::from_rgba8(to.r, to.g, to.b, to.a)),
        ];
        let Some(shader) = LinearGradient::new(p0, p1, stops, SpreadMode::Pad, Transform::identity()) else { return; };
        let mut paint = Paint::default();
        paint.shader = shader;
        paint.anti_alias = true;
        let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
        let r = radius.min(w / 2.0).min(h / 2.0);
        if r < 0.5 {
            if let Some(rr) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                self.pixmap.fill_rect(rr, &paint, Transform::identity(), mask);
            }
        } else if let Some(path) = rounded_rect_path(x, y, w, h, r) {
            self.pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), mask);
        }
        self.has_drawn = true;
    }

    /// Draw a ring segment (progress arc / spinner) by stroking a polyline
    /// approximation of the arc centerline with round caps.
    pub fn fill_arc(&mut self, center: Point, radius: f32, thickness: f32, start_deg: f32, sweep_deg: f32, color: Color) {
        if color.a == 0 || radius < 0.5 || thickness < 0.3 { return; }
        if let Some(clip) = self.clip {
            let r = radius + thickness;
            if !overlaps_clip(center.x - r, center.y - r, r * 2.0, r * 2.0, clip) { return; }
            ensure_clip_mask(&mut self.clip_masks, clip, self.pixmap.width(), self.pixmap.height());
        }
        let segs = ((sweep_deg.abs() / 6.0).ceil() as usize).max(2);
        let mut pb = PathBuilder::new();
        for i in 0..=segs {
            let t = i as f32 / segs as f32;
            let a = (start_deg + sweep_deg * t).to_radians();
            let (px, py) = (center.x + radius * a.cos(), center.y + radius * a.sin());
            if i == 0 { pb.move_to(px, py); } else { pb.line_to(px, py); }
        }
        let Some(path) = pb.finish() else { return; };
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        let stroke = Stroke { width: thickness, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
        let mask = self.clip.and_then(|c| self.clip_masks.get(&c));
        self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
        self.has_drawn = true;
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
                DrawCommand::StrokeRRect { rect, radius, color, width } => {
                    self.stroke_rrect(sr(*rect), *radius * s, *color, *width * s);
                }
                DrawCommand::FillCircle { center, radius, color } => self.fill_circle(sp(*center), *radius * s, *color),
                DrawCommand::FillGradient { rect, radius, from, to, vertical } => self.fill_gradient(sr(*rect), *radius * s, *from, *to, *vertical),
                DrawCommand::FillArc { center, radius, thickness, start_deg, sweep_deg, color } => self.fill_arc(sp(*center), *radius * s, *thickness * s, *start_deg, *sweep_deg, *color),
                DrawCommand::DrawText { text, origin, color, px, weight } => {
                    self.draw_text_weighted(text, sp(*origin), *color, font, *px * s, *weight);
                }
                DrawCommand::DrawShadow { rect, radius, color, blur } => {
                    self.draw_shadow(sr(*rect), *radius * s, *color, *blur * s);
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
    /// `pixels` must be `src_width × src_height × 4` bytes (straight RGBA).
    /// 1:1 blits take a direct row path; scaled blits are sampled bilinearly.
    /// Pixels outside the canvas bounds (and current clip) are skipped.
    pub fn blit_rgba(&mut self, pixels: &[u8], src_w: u32, src_h: u32, dest: Rect) {
        if src_w == 0 || src_h == 0 { return; }
        let cw = self.pixmap.width() as i32;
        let ch = self.pixmap.height() as i32;

        let dx = dest.origin.x.round() as i32;
        let dy = dest.origin.y.round() as i32;
        let dw = dest.size.width.round() as i32;
        let dh = dest.size.height.round() as i32;
        if dw <= 0 || dh <= 0 { return; }

        // Merge canvas bounds with active clip into a single test region.
        let (cx0, cy0, cx1, cy1) = match self.clip {
            Some((cx, cy, cr, cb)) => (cx.max(0), cy.max(0), cr.min(cw), cb.min(ch)),
            None                   => (0, 0, cw, ch),
        };
        if cx1 <= cx0 || cy1 <= cy0 { return; }

        let exact = dw == src_w as i32 && dh == src_h as i32;
        let dst = self.pixmap.data_mut();

        for row in 0..dh {
            let py = dy + row;
            if py < cy0 || py >= cy1 { continue; }
            let row_base = (py * cw) as usize * 4;

            // Vertical source coordinate (bilinear when scaling).
            let (sy0, sy1, wy) = if exact {
                (row as usize, row as usize, 0.0f32)
            } else {
                let fy = ((row as f32 + 0.5) * src_h as f32 / dh as f32 - 0.5)
                    .clamp(0.0, (src_h - 1) as f32);
                let y0 = fy as usize;
                (y0, (y0 + 1).min(src_h as usize - 1), fy - y0 as f32)
            };

            for col in 0..dw {
                let px = dx + col;
                if px < cx0 || px >= cx1 { continue; }

                let (r, g, b, a) = if exact {
                    let si = (sy0 * src_w as usize + col as usize) * 4;
                    (pixels[si] as f32, pixels[si + 1] as f32, pixels[si + 2] as f32, pixels[si + 3] as f32)
                } else {
                    // Bilinear sample of the four surrounding texels.
                    let fx = ((col as f32 + 0.5) * src_w as f32 / dw as f32 - 0.5)
                        .clamp(0.0, (src_w - 1) as f32);
                    let x0 = fx as usize;
                    let x1 = (x0 + 1).min(src_w as usize - 1);
                    let wx = fx - x0 as f32;

                    let idx = |sx: usize, sy: usize| (sy * src_w as usize + sx) * 4;
                    let (i00, i10, i01, i11) = (idx(x0, sy0), idx(x1, sy0), idx(x0, sy1), idx(x1, sy1));
                    let lerp2 = |c: usize| {
                        let top = pixels[i00 + c] as f32 * (1.0 - wx) + pixels[i10 + c] as f32 * wx;
                        let bot = pixels[i01 + c] as f32 * (1.0 - wx) + pixels[i11 + c] as f32 * wx;
                        top * (1.0 - wy) + bot * wy
                    };
                    (lerp2(0), lerp2(1), lerp2(2), lerp2(3))
                };

                let alpha = a as u32;
                if alpha == 0 { continue; }
                let inv = 255 - alpha;
                let di = row_base + px as usize * 4;
                dst[di]     = d255(r as u32 * alpha + dst[di]     as u32 * inv) as u8;
                dst[di + 1] = d255(g as u32 * alpha + dst[di + 1] as u32 * inv) as u8;
                dst[di + 2] = d255(b as u32 * alpha + dst[di + 2] as u32 * inv) as u8;
                dst[di + 3] = 255;
            }
        }
        self.has_drawn = true;
    }

    /// Set (or clear) a master clip in LOGICAL pixels — used for
    /// damage-rect repaints: fills and replays outside it are culled.
    /// `play_picture` treats it as the outer clip and restores it.
    pub fn set_logical_clip(&mut self, r: Option<Rect>) {
        let s = self.scale;
        self.clip = r.map(|r| (
            (r.origin.x * s).floor() as i32,
            (r.origin.y * s).floor() as i32,
            ((r.origin.x + r.size.width) * s).ceil() as i32,
            ((r.origin.y + r.size.height) * s).ceil() as i32,
        ));
    }

    /// Fill a LOGICAL-pixel rect (scaled to physical) — damage background.
    pub fn fill_logical_rect(&mut self, r: Rect, color: Color) {
        let s = self.scale;
        self.fill_rect(Rect {
            origin: Point { x: r.origin.x * s, y: r.origin.y * s },
            size: Size { width: r.size.width * s, height: r.size.height * s },
        }, color);
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
