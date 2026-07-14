//! wgpu GPU compositor for ROSACE (D072–D079).
//!
//! `GpuPresenter` takes one or more RGBA pixel buffers produced by `SkiaCanvas`
//! each frame, uploads them as GPU textures, and composites them onto the wgpu
//! surface:
//! - Pass 1 (base layer): REPLACE blend — overwrites the surface
//! - Pass N (overlay layers): ALPHA_BLENDING — Porter-Duff "over" operation
//!
//! # Integration
//! ```ignore
//! let presenter = GpuPresenter::new(&window, width, height);
//! // in frame loop:
//! presenter.present_layers(&[
//!     CompositorLayer::tracked(base.pixels(), width, height, base_dirty),
//!     CompositorLayer::tracked(overlay.pixels(), width, height, true),
//! ]);
//! ```

use wgpu::util::DeviceExt;

/// Reinterpret a `[f32; 8]` as its raw 32 bytes for GPU upload.
/// SAFETY: `[f32; 8]` is 32 contiguous bytes; every bit pattern is a valid u8.
fn bytemuck_f32x8(data: &[f32; 8]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            std::mem::size_of::<[f32; 8]>(),
        )
    }
}

/// Reinterpret an `&[f32]` as raw bytes for GPU upload (glyph instances).
/// SAFETY: f32s are 4 contiguous bytes each; every bit pattern is a valid u8.
fn f32s_as_bytes(data: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4)
    }
}

/// The glyph-atlas pipeline's WGSL (D109 Step 4): instanced quads, one per
/// glyph — position/size/uv/color per instance, coverage sampled from the
/// R8 atlas, output premultiplied linear.
const GLYPH_WGSL: &str = r#"
struct GlyphGlobals {
    surface_px: vec2<f32>,
    _pad:       vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: GlyphGlobals;
@group(0) @binding(1) var t_atlas: texture_2d<f32>;
@group(0) @binding(2) var s_atlas: sampler;

struct GlyphIn {
    @location(0) pos_px:  vec2<f32>,
    @location(1) size_px: vec2<f32>,
    @location(2) uv_min:  vec2<f32>,
    @location(3) uv_size: vec2<f32>,
    @location(4) color:   vec4<f32>,
};
struct GlyphVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv:    vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, in: GlyphIn) -> GlyphVsOut {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corner[vi];
    let p = in.pos_px + c * in.size_px;
    var out: GlyphVsOut;
    out.pos = vec4<f32>(
        2.0 * p.x / g.surface_px.x - 1.0,
        1.0 - 2.0 * p.y / g.surface_px.y,
        0.0, 1.0,
    );
    out.uv = in.uv_min + c * in.uv_size;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: GlyphVsOut) -> @location(0) vec4<f32> {
    let cov = textureSample(t_atlas, s_atlas, in.uv).r;
    let a = in.color.a * cov;
    return vec4<f32>(in.color.rgb * a, a);
}
"#;

/// One render layer passed to `GpuPresenter::present_layers`.
///
/// `pixels` must be an RGBA8 byte slice of exactly `width * height * 4` bytes.
/// `opacity` scales the entire layer's alpha (1.0 = fully opaque, 0.0 = invisible).
/// `offset` is a UV-space scroll offset `(offset_pixels_x / tex_w, offset_y / tex_h)`.
/// With offset `(0.0, 0.0)` the layer is rendered without scrolling (Phase 15/16 behaviour).
/// Out-of-range UV due to the offset returns transparent (D081).
pub struct CompositorLayer<'a> {
    pub pixels: &'a [u8],
    /// Content-texture dimensions in physical pixels.
    pub width:  u32,
    pub height: u32,
    /// Screen placement in physical pixels. `None` fills the whole surface
    /// (base/overlay); `Some` places the layer at a viewport sub-rect (D090).
    pub dest:   Option<LayerRect>,
    /// Texture sample origin in physical pixels — the scroll offset. `(0,0)`
    /// samples from the top-left (D080).
    pub src_offset: (f32, f32),
    /// True when `pixels` differ from the last frame's for this layer slot.
    /// When false, the compositor reuses the persistent GPU texture and skips
    /// re-upload (D089); when every layer is clean and unmoved, it skips the
    /// present entirely.
    pub dirty:  bool,
}

/// A rectangle in physical pixels (compositor-local, avoids a geometry dep).
#[derive(Clone, Copy, PartialEq)]
pub struct LayerRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// How a registered shader pipeline blends over the frame (D109).
///
/// Compositor-owned mirror of `rosace-shader`'s `BlendMode` — this crate is
/// Layer 0 with a zero-rosace-deps contract, so `rosace-platform` converts
/// at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderBlend {
    /// Premultiplied source-over (the default).
    Alpha,
    /// Replace destination — no blending.
    Opaque,
    /// Source added to destination.
    Additive,
}

/// One GPU shader draw in a presented frame (D109): fill `rect` by running
/// the registered pipeline `pipeline`. All coordinates in physical pixels.
#[derive(Clone, Copy)]
pub struct ShaderQuad<'a> {
    /// Raw pipeline id (a `rosace-shader` `PipelineId::raw()` upstream).
    pub pipeline: u64,
    /// Fill rect: (x, y, w, h).
    pub rect: (f32, f32, f32, f32),
    /// WGSL-uniform-layout bytes for `@group(0) @binding(1)`. Empty is
    /// allowed (a zeroed 16-byte buffer is bound).
    pub uniforms: &'a [u8],
    /// Optional scissor: (x, y, w, h). `None` = unclipped.
    pub clip: Option<(f32, f32, f32, f32)>,
}

/// A reference to an offscreen texture previously rendered via
/// [`GpuPresenter::render_offscreen`] (D109 C2 — GPU scroll layers):
/// sampled at `src_offset` and placed at `dest`, exactly like a placed
/// pixel layer but with content that lives entirely on the GPU.
#[derive(Clone, Copy)]
pub struct OffscreenRef {
    /// The key passed to `render_offscreen` (the scroll layer's node id).
    pub key: u64,
    /// Viewport placement on screen, physical px.
    pub dest: LayerRect,
    /// Texture sample origin in physical px — the live scroll offset.
    pub src_offset: (f32, f32),
    /// True on publish frames (content re-rendered this frame).
    pub dirty: bool,
}

/// One glyph for the atlas pipeline (D109 Step 4). Primitives only (Layer
/// 0): `bitmap` is the coverage mask, read ONLY on the atlas's first sight
/// of `key`; `color` is LINEAR straight-alpha (the platform converts from
/// sRGB — same convention as shape quads). Key bit 63 is RESERVED for
/// color-bitmap glyphs (emoji/COLR, D115) — a future RGBA atlas page keyed
/// by the same map; mask glyphs must keep it 0.
#[derive(Clone, Copy)]
pub struct AtlasGlyph<'a> {
    pub key: u64,
    pub bitmap: &'a [u8],
    /// Top-left, physical px.
    pub x: f32,
    pub y: f32,
    pub w: u32,
    pub h: u32,
    pub color: [f32; 4],
}

/// One image draw (D109 image textures): `pixels` (premultiplied RGBA,
/// `src_w * src_h * 4` bytes) is read ONLY on the cache's first sight of
/// `key` — every later frame is a textured-quad draw with zero CPU pixel
/// work. `opacity` scales the whole quad (Hero fades).
#[derive(Clone, Copy)]
pub struct ImageQuad<'a> {
    pub key: u64,
    pub pixels: &'a [u8],
    pub src_w: u32,
    pub src_h: u32,
    /// Dest rect (x, y, w, h), physical px.
    pub dest: (f32, f32, f32, f32),
    pub opacity: f32,
    /// Optional scissor (x, y, w, h), physical px.
    pub clip: Option<(f32, f32, f32, f32)>,
}

/// One item of a presented frame, drawn strictly in slice order —
/// bottom-to-top z. Pixel layers keep their D089 persistent-texture cache;
/// shader quads execute their registered pipeline directly on the surface;
/// offscreen refs sample a texture rendered via `render_offscreen`; glyph
/// batches draw as instanced quads over the glyph atlas; images draw from
/// the content-keyed texture cache.
pub enum FrameItem<'a> {
    Pixels(CompositorLayer<'a>),
    Shader(ShaderQuad<'a>),
    Offscreen(OffscreenRef),
    Glyphs { glyphs: Vec<AtlasGlyph<'a>>, clip: Option<(f32, f32, f32, f32)> },
    Image(ImageQuad<'a>),
    Backdrop(BackdropQuad),
}

/// A frosted-glass panel (D-DEF-012): everything drawn BEFORE this item is
/// blurred within `rect` and composited back as a tinted rounded panel.
/// Frames containing one render through an intermediate scene texture
/// (zero cost for frames without any). All physical px.
#[derive(Clone, Copy, PartialEq)]
pub struct BackdropQuad {
    pub rect: (f32, f32, f32, f32),
    pub radius: f32,
    /// Gaussian strength in physical px.
    pub blur: f32,
    /// LINEAR straight-alpha tint; alpha = mix strength over the blur.
    pub tint: [f32; 4],
}

/// Separable 9-tap Gaussian, run twice (H then V) at half resolution.
const BLUR_WGSL: &str = r#"
struct BlurUniform {
    // xy = one texel in UV units, zw = blur direction (1,0) or (0,1),
    texel_dir: vec4<f32>,
    // x = tap spread multiplier (blur strength / 3).
    params:    vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: BlurUniform;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var s_src: sampler;

struct BlurVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> BlurVsOut {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corner[vi];
    var out: BlurVsOut;
    out.pos = vec4<f32>(c.x * 2.0 - 1.0, 1.0 - c.y * 2.0, 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs_main(in: BlurVsOut) -> @location(0) vec4<f32> {
    let step = u.texel_dir.xy * u.texel_dir.zw * u.params.x;
    var acc = textureSample(t_src, s_src, in.uv) * 0.227027;
    var w = array<f32, 4>(0.1945946, 0.1216216, 0.054054, 0.016216);
    for (var i = 1; i <= 4; i++) {
        let o = step * f32(i);
        acc += textureSample(t_src, s_src, in.uv + o) * w[i - 1];
        acc += textureSample(t_src, s_src, in.uv - o) * w[i - 1];
    }
    return acc;
}
"#;

/// The glass panel: samples the blurred scene at the panel's screen UVs,
/// tints it, masks to a rounded rect with a soft rim.
const GLASS_WGSL: &str = r#"
struct GlassUniform {
    dest_min: vec2<f32>,   // NDC
    dest_max: vec2<f32>,
    uv_min:   vec2<f32>,   // panel rect in screen UV (samples the blur tex)
    uv_span:  vec2<f32>,
    size_px:  vec2<f32>,   // panel size, physical px
    radius:   vec2<f32>,   // x = corner radius px
    tint:     vec4<f32>,   // linear straight-alpha; a = mix strength
};
@group(0) @binding(0) var<uniform> u: GlassUniform;
@group(0) @binding(1) var t_blur: texture_2d<f32>;
@group(0) @binding(2) var s_blur: sampler;

struct GlassVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

fn sd_rrect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> GlassVsOut {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corner[vi];
    var out: GlassVsOut;
    out.pos = vec4<f32>(
        mix(u.dest_min.x, u.dest_max.x, c.x),
        mix(u.dest_max.y, u.dest_min.y, c.y),
        0.0, 1.0,
    );
    out.local = c;
    out.uv = u.uv_min + c * u.uv_span;
    return out;
}

@fragment
fn fs_main(in: GlassVsOut) -> @location(0) vec4<f32> {
    let p = in.local * u.size_px - u.size_px * 0.5;
    let d = sd_rrect(p, u.size_px * 0.5, u.radius.x);
    let mask = clamp(0.5 - d, 0.0, 1.0);
    if mask <= 0.0 { return vec4<f32>(0.0); }

    let blurred = textureSample(t_blur, s_blur, in.uv).rgb;
    // Tint over the blur, slight lift so glass reads brighter than what's
    // behind it, and a soft rim at the panel edge.
    var col = mix(blurred, u.tint.rgb, u.tint.a) * 1.04 + vec3<f32>(0.015);
    let rim = smoothstep(-3.0, -0.5, d) * 0.25;
    col += vec3<f32>(rim);
    return vec4<f32>(col * mask, mask);
}
"#;

/// Full-resolution scene + half-resolution blur ping-pong targets, created
/// lazily on the first backdrop frame and resized with the surface.
struct SceneTargets {
    #[allow(dead_code)]
    scene:       wgpu::Texture,
    scene_view:  wgpu::TextureView,
    half_a_view: wgpu::TextureView,
    half_b_view: wgpu::TextureView,
    /// Blits the finished scene to the surface (pipeline_base).
    blit_bind_group: wgpu::BindGroup,
    w: u32,
    h: u32,
}

/// The image-quad pipeline's WGSL (D109): one quad, dest in NDC via
/// uniform, texture sampled bilinearly, whole output scaled by opacity
/// (premultiplied fade — matches the CPU blit's alpha-scale semantics).
const IMAGE_WGSL: &str = r#"
struct ImgUniform {
    dest_min: vec2<f32>,
    dest_max: vec2<f32>,
    // x = opacity; yzw unused (16-byte alignment).
    params:   vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: ImgUniform;
@group(0) @binding(1) var t_img: texture_2d<f32>;
@group(0) @binding(2) var s_img: sampler;

struct ImgVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> ImgVsOut {
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corner[vi];
    var out: ImgVsOut;
    let ndc_x = mix(u.dest_min.x, u.dest_max.x, c.x);
    let ndc_y = mix(u.dest_max.y, u.dest_min.y, c.y);
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs_main(in: ImgVsOut) -> @location(0) vec4<f32> {
    return textureSample(t_img, s_img, in.uv) * u.params.x;
}
"#;

/// A cached image texture (D109): uploaded once per distinct content key.
/// Holds only the texture — the dest/opacity uniform is a per-DRAW
/// transient (a per-texture uniform would break when the same image
/// appears at two dest rects in one frame, e.g. a repeated avatar).
struct ImageTexEntry {
    #[allow(dead_code)] // owns the view's backing texture
    texture:   wgpu::Texture,
    view:      wgpu::TextureView,
    /// GPU size of this texture (w * h * 4).
    bytes:     usize,
    /// The present sequence number that last drew this image — the LRU
    /// ordering for byte-budget eviction.
    last_used: u64,
}

/// Byte budget for the image-texture cache. When exceeded after a
/// present, least-recently-USED off-screen entries are evicted until back
/// under budget; on-screen entries are never evicted. 256MB comfortably
/// holds dozens of full-screen Retina images while bounding VRAM.
const IMAGE_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;

impl<'a> CompositorLayer<'a> {
    /// Convenience: full-surface opaque layer, always re-uploaded.
    pub fn opaque(pixels: &'a [u8], width: u32, height: u32) -> Self {
        Self { pixels, width, height, dest: None, src_offset: (0.0, 0.0), dirty: true }
    }

    /// Full-surface layer that only re-uploads its texture when `dirty` (D089).
    pub fn tracked(pixels: &'a [u8], width: u32, height: u32, dirty: bool) -> Self {
        Self { pixels, width, height, dest: None, src_offset: (0.0, 0.0), dirty }
    }

    /// A layer placed at a viewport sub-rect (`dest`, physical px) that samples
    /// its content texture from `src_offset` — a scrolling content layer (D090).
    pub fn placed(
        pixels: &'a [u8], width: u32, height: u32,
        dest: LayerRect, src_offset: (f32, f32), dirty: bool,
    ) -> Self {
        Self { pixels, width, height, dest: Some(dest), src_offset, dirty }
    }

    /// Compute the 8-float uniform (dest NDC + UV window) for this layer given
    /// the current surface size in physical pixels.
    fn uniform(&self, surface_w: u32, surface_h: u32) -> [f32; 8] {
        placed_uniform(self.dest, self.width, self.height, self.src_offset, surface_w, surface_h)
    }
}

/// Dest NDC + UV window for a placed texture — shared by pixel layers and
/// offscreen refs (D109 C2). Pure function.
fn placed_uniform(
    dest: Option<LayerRect>, tex_w: u32, tex_h: u32,
    src_offset: (f32, f32), surface_w: u32, surface_h: u32,
) -> [f32; 8] {
    let (sw, sh) = (surface_w.max(1) as f32, surface_h.max(1) as f32);
    let (tw, th) = (tex_w.max(1) as f32, tex_h.max(1) as f32);

    // Screen placement → NDC. Full surface when dest is None.
    let d = dest.unwrap_or(LayerRect { x: 0.0, y: 0.0, w: sw, h: sh });
    let ndc_left   = 2.0 * d.x / sw - 1.0;
    let ndc_right  = 2.0 * (d.x + d.w) / sw - 1.0;
    let ndc_top    = 1.0 - 2.0 * d.y / sh;
    let ndc_bottom = 1.0 - 2.0 * (d.y + d.h) / sh;

    // UV window: sample a d.w × d.h region of the texture starting at the
    // scroll offset (1:1 physical px → texel, no scaling).
    let uv_min_x  = src_offset.0 / tw;
    let uv_min_y  = src_offset.1 / th;
    let uv_span_x = d.w / tw;
    let uv_span_y = d.h / th;

    [
        ndc_left, ndc_bottom,   // dest_min
        ndc_right, ndc_top,     // dest_max
        uv_min_x, uv_min_y,     // uv_min
        uv_span_x, uv_span_y,   // uv_span
    ]
}

/// Atlas size — 2048² of R8 coverage holds thousands of UI-size glyphs.
/// Growth/eviction is NAMED FUTURE WORK (a full atlas warns loudly and
/// skips new glyphs), not silently assumed away.
const GLYPH_ATLAS_DIM: u32 = 2048;

/// One packed glyph in the atlas: its texel rect.
#[derive(Clone, Copy)]
struct AtlasSlot {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Shelf packer over the atlas texture: rows of similar-height glyphs,
/// 1px gutters against sampling bleed. Pure allocator — unit-testable.
struct ShelfPacker {
    shelves: Vec<(u32, u32, u32)>, // (y, height, cursor_x)
    next_y: u32,
    dim: u32,
}

impl ShelfPacker {
    fn new(dim: u32) -> Self {
        Self { shelves: Vec::new(), next_y: 0, dim }
    }

    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w + 2 > self.dim || h + 2 > self.dim { return None; }
        let (w2, h2) = (w + 1, h + 1); // 1px gutter right/below
        for (sy, sh, cursor) in self.shelves.iter_mut() {
            if h2 <= *sh && *sh <= h2 + h2 / 2 && *cursor + w2 <= self.dim {
                let pos = (*cursor, *sy);
                *cursor += w2;
                return Some(pos);
            }
        }
        if self.next_y + h2 > self.dim { return None; }
        let y = self.next_y;
        self.next_y += h2;
        self.shelves.push((y, h2, w2));
        Some((0, y))
    }
}

/// The glyph atlas (D109 Step 4): coverage masks uploaded once per
/// distinct key, sampled as instanced quads forever after.
struct GlyphAtlas {
    texture:    wgpu::Texture,
    view:       wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    packer:     ShelfPacker,
    slots:      std::collections::HashMap<u64, AtlasSlot>,
    /// Set when the atlas was flushed during THIS present's prepare pass —
    /// instance floats built before the flush hold stale UVs and must be
    /// rebuilt (see present_frame / render_offscreen).
    flushed:    bool,
    /// Warned-once flag: a single frame's working set exceeds the atlas.
    full_warned: bool,
}

/// An offscreen render target (D109 C2): scroll-layer content rendered
/// once per publish, sampled with a live UV offset every frame after.
struct OffscreenLayer {
    texture:     wgpu::Texture,
    bind_group:  wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    width:       u32,
    height:      u32,
    /// Last placed uniform written — compared to skip writes/presents.
    uniform:     [f32; 8],
}

/// One persistent GPU texture + bind group for a layer slot, reused across
/// frames so clean frames pay no upload cost (D089).
struct CachedLayer {
    texture:     wgpu::Texture,
    bind_group:  wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    width:       u32,
    height:      u32,
    /// Last uniform written (dest NDC + UV window) — compared to skip both the
    /// present and the uniform write when nothing moved.
    uniform:     [f32; 8],
}

/// A registered shader pipeline (D109) — compiled eagerly at
/// `register_shader` time, never lazily on first paint (the Impeller
/// lesson, PHASE_27.md).
struct ShaderPipelineEntry {
    pipeline: wgpu::RenderPipeline,
}

/// Persistent GPU resources for one shader quad slot, reused across frames
/// (same D089 discipline as pixel layers): buffers rewritten only when the
/// quad's rect/uniforms change, bind group rebuilt only on uniform-size
/// change.
struct CachedShaderQuad {
    pipeline:    u64,
    rect:        (f32, f32, f32, f32),
    uniforms:    Vec<u8>,
    clip:        Option<(f32, f32, f32, f32)>,
    quad_buf:    wgpu::Buffer,
    user_buf:    wgpu::Buffer,
    user_len:    usize,
    bind_group:  wgpu::BindGroup,
}

/// Compute the quad placement uniform (dest NDC + size in px) for a shader
/// quad. Pure function — unit-testable without a GPU.
fn shader_quad_uniform(rect: (f32, f32, f32, f32), surface_w: u32, surface_h: u32) -> [f32; 8] {
    let (sw, sh) = (surface_w.max(1) as f32, surface_h.max(1) as f32);
    let (x, y, w, h) = rect;
    [
        2.0 * x / sw - 1.0,           // dest_min.x (left, NDC)
        1.0 - 2.0 * (y + h) / sh,     // dest_min.y (bottom, NDC)
        2.0 * (x + w) / sw - 1.0,     // dest_max.x (right, NDC)
        1.0 - 2.0 * y / sh,           // dest_max.y (top, NDC)
        w, h,                          // size_px
        0.0, 0.0,                      // pad
    ]
}

/// Clamp a physical-px clip rect to the surface and convert to a wgpu
/// scissor `(x, y, w, h)`. `None` when the intersection is empty (the quad
/// draws nothing). Pure function — unit-testable without a GPU.
fn scissor_for(clip: (f32, f32, f32, f32), surface_w: u32, surface_h: u32) -> Option<(u32, u32, u32, u32)> {
    let (x, y, w, h) = clip;
    let x0 = x.max(0.0).floor() as u32;
    let y0 = y.max(0.0).floor() as u32;
    let x1 = ((x + w).min(surface_w as f32).ceil() as u32).min(surface_w);
    let y1 = ((y + h).min(surface_h as f32).ceil() as u32).min(surface_h);
    if x1 > x0 && y1 > y0 { Some((x0, y0, x1 - x0, y1 - y0)) } else { None }
}

/// One prepared glyph batch for the skip-present comparison (D089):
/// (clip rect, instance floats).
type GlyphBatchData = (Option<(f32, f32, f32, f32)>, Vec<f32>);

/// Skip-present comparison entry for one image draw (D089):
/// (content key, dest rect, opacity, clip rect).
type ImageDrawData = (u64, (f32, f32, f32, f32), f32, Option<(f32, f32, f32, f32)>);

/// GPU compositor state. One instance per window.
///
/// Created via [`GpuPresenter::new`]. Returns `None` if wgpu fails to find a
/// compatible GPU adapter; callers should fall back to the softbuffer path.
pub struct GpuPresenter {
    surface:               wgpu::Surface<'static>,
    device:                wgpu::Device,
    queue:                 wgpu::Queue,
    config:                wgpu::SurfaceConfiguration,
    /// Pipeline for the base layer (REPLACE blend — writes all channels).
    pipeline_base:         wgpu::RenderPipeline,
    /// Pipeline for overlay layers (ALPHA_BLENDING — Porter-Duff over).
    pipeline_overlay:      wgpu::RenderPipeline,
    bind_group_layout:     wgpu::BindGroupLayout,
    sampler:               wgpu::Sampler,
    width:                 u32,
    height:                u32,
    /// Persistent per-slot textures reused across frames (D089).
    cached_layers:         Vec<CachedLayer>,
    /// Registered shader pipelines keyed by raw pipeline id (D109) —
    /// compiled once at registration, stable resources per D091 discipline.
    shader_pipelines:      std::collections::HashMap<u64, ShaderPipelineEntry>,
    /// Bind group layout shared by every shader pipeline: binding 0 = quad
    /// placement uniform (vertex+fragment), binding 1 = user uniforms
    /// (fragment).
    shader_bgl:            wgpu::BindGroupLayout,
    shader_pipeline_layout: wgpu::PipelineLayout,
    /// Persistent per-slot quad resources (D089 discipline for quads).
    cached_quads:          Vec<CachedShaderQuad>,
    /// Offscreen render targets keyed by caller key (D109 C2 — GPU scroll
    /// layers). Unreferenced keys are evicted after each full present.
    offscreen:             std::collections::HashMap<u64, OffscreenLayer>,
    /// The glyph atlas + its instanced pipeline (D109 Step 4).
    glyph_atlas:           GlyphAtlas,
    glyph_pipeline:        wgpu::RenderPipeline,
    glyph_bgl:             wgpu::BindGroupLayout,
    glyph_globals_buf:     wgpu::Buffer,
    /// Last surface size written to the glyph globals uniform.
    glyph_globals_size:    (u32, u32),
    /// Coverage gamma LUT applied at atlas upload — set by the platform
    /// from the render crate's curve so both text paths share one source.
    glyph_gamma:           Option<&'static [u8; 256]>,
    /// Per-slot cached glyph batches (instance bytes + clip) — the
    /// skip-present comparison for text, mirroring cached_quads.
    cached_glyph_batches:  Vec<GlyphBatchData>,
    /// Image textures keyed by content hash (D109 image textures).
    image_cache:           std::collections::HashMap<u64, ImageTexEntry>,
    image_pipeline:        wgpu::RenderPipeline,
    image_bgl:             wgpu::BindGroupLayout,
    /// Bilinear sampler — images scale to their dest rect (unlike layers
    /// and glyphs, which are 1:1 and use the nearest sampler).
    sampler_linear:        wgpu::Sampler,
    /// Skip-present comparison for images: (key, dest, opacity, clip).
    cached_images:         Vec<ImageDrawData>,
    /// Total bytes held by `image_cache` (kept in sync on insert/evict).
    image_cache_bytes:     usize,
    /// Monotonic present counter — the LRU clock for image eviction.
    present_seq:           u64,
    /// Warned-once flag: visible images alone exceed the byte budget.
    image_budget_warned:   bool,
    /// Backdrop-blur pipelines + scene targets (D-DEF-012). Targets are
    /// lazy — frames without a Backdrop item never allocate them.
    blur_pipeline:         wgpu::RenderPipeline,
    glass_pipeline:        wgpu::RenderPipeline,
    scene:                 Option<SceneTargets>,
    /// Skip-present comparison for backdrops.
    cached_backdrops:      Vec<BackdropQuad>,
    /// Pipeline ids already warned about as unregistered — warn once, not
    /// per frame.
    missing_pipeline_warned: std::collections::HashSet<u64>,
}

impl GpuPresenter {
    /// Initialise the GPU presenter for the given window handle.
    ///
    /// Blocks using `pollster`. Returns `None` if no suitable adapter is found.
    pub fn new<W>(window: W, width: u32, height: u32) -> Option<Self>
    where
        W: wgpu::rwh::HasWindowHandle
            + wgpu::rwh::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        pollster::block_on(Self::new_async(window, width, height))
    }

    async fn new_async<W>(window: W, width: u32, height: u32) -> Option<Self>
    where
        W: wgpu::rwh::HasWindowHandle
            + wgpu::rwh::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).ok()?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        log::info!(
            "wgpu: {} backend, adapter = {}",
            adapter.get_info().backend,
            adapter.get_info().name,
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label:             Some("rosace-compositor"),
                required_features: wgpu::Features::empty(),
                required_limits:   wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints:      Default::default(),
            }, None)
            .await
            .ok()?;

        let caps   = surface.get_capabilities(&adapter);
        let format = caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:        width.max(1),
            height:       height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode:   caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("compositor"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("frame-texture-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Binding 2: LayerUniforms (dest NDC + UV window, 32 bytes) (D090).
                // Read in the VERTEX stage (quad placement) — must be visible there.
                wgpu::BindGroupLayoutEntry {
                    binding:    2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("compositor-pl"),
            bind_group_layouts:   &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Base layer pipeline — REPLACE blend (first pass, writes everything)
        let pipeline_base = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("compositor-base"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs_main"),
                buffers:             &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // Overlay pipeline — Porter-Duff "over" (subsequent passes, alpha blend)
        let pipeline_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("compositor-overlay"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs_main"),
                buffers:             &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter:     wgpu::FilterMode::Nearest,
            min_filter:     wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Shared layout for registered shader pipelines (D109): quad
        // placement uniform + user uniform bytes. `min_binding_size: None`
        // because user uniform structs vary per pipeline.
        let shader_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("shader-quad-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   None,
                    },
                    count: None,
                },
            ],
        });
        let shader_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("shader-quad-pl"),
            bind_group_layouts:   &[&shader_bgl],
            push_constant_ranges: &[],
        });

        // ── Glyph atlas + instanced pipeline (D109 Step 4) ──────────────
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("glyph-atlas"),
            size:            wgpu::Extent3d {
                width: GLYPH_ATLAS_DIM, height: GLYPH_ATLAS_DIM, depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::R8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let glyph_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("glyph-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let glyph_globals = [width as f32, height as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let glyph_globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("glyph-globals"),
            contents: bytemuck_f32x8(&glyph_globals),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("glyph-atlas-bg"),
            layout:  &glyph_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: glyph_globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let glyph_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("glyph-pipeline"),
            source: wgpu::ShaderSource::Wgsl(GLYPH_WGSL.into()),
        });
        let glyph_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("glyph-pl"),
            bind_group_layouts:   &[&glyph_bgl],
            push_constant_ranges: &[],
        });
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("glyph-pipeline"),
            layout: Some(&glyph_pl),
            vertex: wgpu::VertexState {
                module:      &glyph_module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 48, // 12 f32s per instance
                    step_mode:    wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x2,
                        2 => Float32x2, 3 => Float32x2,
                        4 => Float32x4,
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &glyph_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // ── Image-quad pipeline (D109 image textures) ───────────────────
        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter:     wgpu::FilterMode::Linear,
            min_filter:     wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let image_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("image-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let image_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("image-pipeline"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_WGSL.into()),
        });
        let image_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("image-pl"),
            bind_group_layouts:   &[&image_bgl],
            push_constant_ranges: &[],
        });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("image-pipeline"),
            layout: Some(&image_pl),
            vertex: wgpu::VertexState {
                module:              &image_module,
                entry_point:         Some("vs_main"),
                buffers:             &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &image_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // Backdrop blur + glass pipelines (D-DEF-012) — same bind layout
        // shape as images (uniform + texture + sampler), so image_bgl is
        // reused for both.
        let mk_pipeline = |label: &str, src: &str| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label:  Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label:                Some(label),
                bind_group_layouts:   &[&image_bgl],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label:  Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module, entry_point: Some("vs_main"),
                    buffers: &[], compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module, entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive:     wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample:   wgpu::MultisampleState::default(),
                multiview:     None,
                cache:         None,
            })
        };
        let blur_pipeline  = mk_pipeline("backdrop-blur", BLUR_WGSL);
        let glass_pipeline = mk_pipeline("backdrop-glass", GLASS_WGSL);

        Some(Self {
            surface,
            device,
            queue,
            config,
            pipeline_base,
            pipeline_overlay,
            bind_group_layout,
            sampler,
            width,
            height,
            cached_layers: Vec::new(),
            shader_pipelines: std::collections::HashMap::new(),
            shader_bgl,
            shader_pipeline_layout,
            cached_quads: Vec::new(),
            offscreen: std::collections::HashMap::new(),
            glyph_atlas: GlyphAtlas {
                texture:     atlas_texture,
                view:        atlas_view,
                bind_group:  atlas_bind_group,
                packer:      ShelfPacker::new(GLYPH_ATLAS_DIM),
                slots:       std::collections::HashMap::new(),
                flushed:     false,
                full_warned: false,
            },
            glyph_pipeline,
            glyph_bgl,
            glyph_globals_buf,
            glyph_globals_size: (width, height),
            glyph_gamma: None,
            cached_glyph_batches: Vec::new(),
            image_cache: std::collections::HashMap::new(),
            image_cache_bytes: 0,
            present_seq: 0,
            image_budget_warned: false,
            image_pipeline,
            image_bgl,
            sampler_linear,
            cached_images: Vec::new(),
            blur_pipeline,
            glass_pipeline,
            scene: None,
            cached_backdrops: Vec::new(),
            missing_pipeline_warned: std::collections::HashSet::new(),
        })
    }

    /// Ensure the scene + half-res blur targets exist at the surface size.
    fn ensure_scene(&mut self, sw: u32, sh: u32) {
        if self.scene.as_ref().map(|t| t.w == sw && t.h == sh).unwrap_or(false) {
            return;
        }
        let mk = |w: u32, h: u32, label: &str| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label:           Some(label),
                size:            wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          self.config.format,
                usage:           wgpu::TextureUsages::RENDER_ATTACHMENT
                               | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats:    &[],
            })
        };
        let scene = mk(sw, sh, "backdrop-scene");
        let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());
        let half_a = mk(sw / 2, sh / 2, "backdrop-half-a");
        let half_b = mk(sw / 2, sh / 2, "backdrop-half-b");
        // Fullscreen blit of the finished scene onto the surface: reuse the
        // layer machinery (pipeline_base + bind_group_layout).
        let uniform = placed_uniform(None, sw, sh, (0.0, 0.0), sw, sh);
        let blit_uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("scene-blit-uniform"),
            contents: bytemuck_f32x8(&uniform),
            usage:    wgpu::BufferUsages::UNIFORM,
        });
        let blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("scene-blit-bg"),
            layout:  &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&scene_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: blit_uniform.as_entire_binding() },
            ],
        });
        self.scene = Some(SceneTargets {
            scene, scene_view,
            half_a_view: half_a.create_view(&wgpu::TextureViewDescriptor::default()),
            half_b_view: half_b.create_view(&wgpu::TextureViewDescriptor::default()),
            blit_bind_group,
            w: sw, h: sh,
        });
    }

    /// Ensure `q`'s texture exists in the image cache (uploading on first
    /// sight of the key) and return whether it's usable.
    fn ensure_image_texture(&mut self, q: &ImageQuad<'_>) -> bool {
        if q.src_w == 0 || q.src_h == 0 { return false; }
        if !self.image_cache.contains_key(&q.key) {
            let expected = (q.src_w * q.src_h * 4) as usize;
            if q.pixels.len() < expected {
                log::error!(
                    "image {:#x}: {} pixel bytes for {}x{} (need {expected}) — skipped",
                    q.key, q.pixels.len(), q.src_w, q.src_h,
                );
                return false;
            }
            let texture = self.device.create_texture_with_data(
                &self.queue,
                &wgpu::TextureDescriptor {
                    label:           Some("image-texture"),
                    size:            wgpu::Extent3d {
                        width: q.src_w, height: q.src_h, depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count:    1,
                    dimension:       wgpu::TextureDimension::D2,
                    format:          wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage:           wgpu::TextureUsages::TEXTURE_BINDING
                                   | wgpu::TextureUsages::COPY_DST,
                    view_formats:    &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &q.pixels[..expected],
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.image_cache.insert(q.key, ImageTexEntry {
                texture, view, bytes: expected, last_used: self.present_seq,
            });
            self.image_cache_bytes += expected;
        }
        if let Some(e) = self.image_cache.get_mut(&q.key) { e.last_used = self.present_seq; }
        true
    }

    /// Per-draw transient uniform + bind group for an image quad against a
    /// `(target_w, target_h)` render target. `None` if the texture is
    /// missing/unusable.
    fn image_draw_bind_group(
        &mut self, q: &ImageQuad<'_>, target_w: u32, target_h: u32,
    ) -> Option<wgpu::BindGroup> {
        if !self.ensure_image_texture(q) { return None; }
        let ndc = shader_quad_uniform(q.dest, target_w, target_h);
        let uniform = [ndc[0], ndc[1], ndc[2], ndc[3], q.opacity, 0.0, 0.0, 0.0];
        let uniform_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("image-uniform"),
            contents: bytemuck_f32x8(&uniform),
            usage:    wgpu::BufferUsages::UNIFORM,
        });
        let entry = &self.image_cache[&q.key];
        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("image-bg"),
            layout:  &self.image_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&entry.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler_linear) },
            ],
        }))
    }

    /// Set the coverage-gamma LUT applied at glyph upload (D109 Step 4).
    /// The platform passes the render crate's `text_gamma_lut()` so the
    /// CPU blit path and the atlas share ONE curve — this Layer-0 crate
    /// cannot import it.
    pub fn set_glyph_gamma(&mut self, lut: &'static [u8; 256]) {
        self.glyph_gamma = Some(lut);
    }

    /// Allocate an atlas slot, FLUSHING the atlas when full (Skia-style
    /// glyph-cache strategy): clear every slot and let the current working
    /// set re-upload lazily — every batch carries its bitmaps, so
    /// repopulation is automatic, and there's no shelf-fragmentation
    /// bookkeeping. Instances built before a flush hold stale UVs; the
    /// `flushed` flag makes the caller rebuild them. Returns `None` only
    /// when a single frame's working set (or one glyph) exceeds the atlas.
    fn atlas_alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if let Some(pos) = self.glyph_atlas.packer.alloc(w, h) {
            return Some(pos);
        }
        if !self.glyph_atlas.slots.is_empty() {
            log::info!(
                "glyph atlas full — flushing {} glyphs; current working set re-uploads",
                self.glyph_atlas.slots.len(),
            );
            self.glyph_atlas.slots.clear();
            self.glyph_atlas.packer = ShelfPacker::new(GLYPH_ATLAS_DIM);
            self.glyph_atlas.flushed = true;
            return self.glyph_atlas.packer.alloc(w, h);
        }
        None
    }

    /// Build one offscreen glyph batch's transient resources: instances
    /// against the SHARED atlas, globals sized to the TARGET (not the
    /// surface). `None` = nothing to draw for this batch.
    #[allow(clippy::type_complexity)]
    fn offscreen_glyph_transient(
        &mut self,
        glyphs: &[AtlasGlyph<'_>],
        clip: Option<(f32, f32, f32, f32)>,
        width: u32,
        height: u32,
    ) -> Option<(wgpu::BindGroup, wgpu::Buffer, u32, Option<(u32, u32, u32, u32)>)> {
        let scissor = match clip {
            Some(c) => Some(scissor_for(c, width, height)?),
            None => None,
        };
        let instances = self.prepare_glyphs(glyphs);
        if instances.is_empty() { return None; }
        let globals = [width as f32, height as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let globals_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("offscreen-glyph-globals"),
            contents: bytemuck_f32x8(&globals),
            usage:    wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("offscreen-glyph-bg"),
            layout:  &self.glyph_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.glyph_atlas.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("offscreen-glyph-instances"),
            contents: f32s_as_bytes(&instances),
            usage:    wgpu::BufferUsages::VERTEX,
        });
        Some((bind_group, inst_buf, (instances.len() / 12) as u32, scissor))
    }

    /// Upload any first-seen glyphs to the atlas and build the per-batch
    /// instance floats (12 per glyph). Atlas-full is loud-once + skip.
    fn prepare_glyphs(&mut self, glyphs: &[AtlasGlyph<'_>]) -> Vec<f32> {
        let mut out: Vec<f32> = Vec::with_capacity(glyphs.len() * 12);
        let inv_dim = 1.0 / GLYPH_ATLAS_DIM as f32;
        for g in glyphs {
            if !self.glyph_atlas.slots.contains_key(&g.key) {
                let Some((ax, ay)) = self.atlas_alloc(g.w, g.h) else {
                    if !self.glyph_atlas.full_warned {
                        self.glyph_atlas.full_warned = true;
                        log::error!(
                            "glyph atlas exhausted even after a flush ({} slots) — a \
                             single frame's glyph working set exceeds {}x{}; those \
                             glyphs will not render",
                            self.glyph_atlas.slots.len(),
                            GLYPH_ATLAS_DIM, GLYPH_ATLAS_DIM,
                        );
                    }
                    continue;
                };
                // Apply the text gamma curve once, at upload.
                let bytes: Vec<u8> = match self.glyph_gamma {
                    Some(lut) => g.bitmap.iter().map(|&b| lut[b as usize]).collect(),
                    None      => g.bitmap.to_vec(),
                };
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture:   &self.glyph_atlas.texture,
                        mip_level: 0,
                        origin:    wgpu::Origin3d { x: ax, y: ay, z: 0 },
                        aspect:    wgpu::TextureAspect::All,
                    },
                    &bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset:         0,
                        bytes_per_row:  Some(g.w),
                        rows_per_image: Some(g.h),
                    },
                    wgpu::Extent3d { width: g.w, height: g.h, depth_or_array_layers: 1 },
                );
                self.glyph_atlas.slots.insert(g.key, AtlasSlot { x: ax, y: ay, w: g.w, h: g.h });
            }
            let Some(slot) = self.glyph_atlas.slots.get(&g.key) else { continue; };
            out.extend_from_slice(&[
                g.x, g.y,
                g.w as f32, g.h as f32,
                slot.x as f32 * inv_dim, slot.y as f32 * inv_dim,
                slot.w as f32 * inv_dim, slot.h as f32 * inv_dim,
                g.color[0], g.color[1], g.color[2], g.color[3],
            ]);
        }
        out
    }

    /// Render `items` into the offscreen texture for `key` (D109 C2 — GPU
    /// scroll-layer content). Called on publish frames only; every frame
    /// after samples the texture at the live scroll offset via
    /// [`FrameItem::Offscreen`]. The target is cleared to transparent
    /// first — uncovered areas reveal whatever is beneath the layer.
    ///
    /// Resources here are transient by design: publishes happen on repaint
    /// frames (rare relative to scrolled frames), so per-publish buffer
    /// creation is the right cost/complexity trade until measured otherwise.
    pub fn render_offscreen(&mut self, key: u64, width: u32, height: u32, items: &[FrameItem<'_>]) {
        let (width, height) = (width.max(1), height.max(1));

        // Reuse the target texture when dimensions match; else recreate
        // (and its sampling bind group + uniform buffer).
        let dims_match = self.offscreen.get(&key)
            .map(|o| o.width == width && o.height == height)
            .unwrap_or(false);
        if !dims_match {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label:           Some("offscreen-scroll"),
                size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                // MUST be the surface's format: every registered pipeline
                // and the layer pipelines compile against config.format,
                // and a render pass validates attachment formats against
                // the pipeline (this was a real launch abort with
                // Rgba8UnormSrgb vs the macOS Bgra8UnormSrgb surface).
                // Sampling doesn't care about component order.
                format:          self.config.format,
                usage:           wgpu::TextureUsages::RENDER_ATTACHMENT
                               | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats:    &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let uniform = [0.0f32; 8]; // real value written at present time
            let uniform_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:    Some("offscreen-uniforms"),
                contents: bytemuck_f32x8(&uniform),
                usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label:   Some("offscreen-bg"),
                layout:  &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform_buf.as_entire_binding() },
                ],
            });
            self.offscreen.insert(key, OffscreenLayer {
                texture, bind_group, uniform_buf, width, height, uniform,
            });
        }

        let target_view = self.offscreen[&key].texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen-enc"),
        });

        // Transient per-item resources, dropped after submit (see doc note).
        let mut transient_layers: Vec<CachedLayer> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut transient_quads:  Vec<(u64, wgpu::BindGroup, Option<(u32, u32, u32, u32)>)> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut transient_glyphs: Vec<Option<(wgpu::BindGroup, wgpu::Buffer, u32, Option<(u32, u32, u32, u32)>)>> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut transient_images: Vec<Option<(wgpu::BindGroup, Option<(u32, u32, u32, u32)>)>> = Vec::new();
        for item in items {
            match item {
                FrameItem::Pixels(layer) => {
                    if layer.width == 0 || layer.height == 0 { continue; }
                    transient_layers.push(self.build_cached_layer(layer, width, height));
                }
                FrameItem::Shader(quad) => {
                    let scissor = match quad.clip {
                        Some(c) => match scissor_for(c, width, height) {
                            Some(s) => Some(s),
                            None => continue,
                        },
                        None => None,
                    };
                    const EMPTY_UNIFORMS: [u8; 16] = [0u8; 16];
                    let user_bytes: &[u8] =
                        if quad.uniforms.is_empty() { &EMPTY_UNIFORMS } else { quad.uniforms };
                    let quad_uniform = shader_quad_uniform(quad.rect, width, height);
                    let quad_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label:    Some("offscreen-quad-uniform"),
                        contents: bytemuck_f32x8(&quad_uniform),
                        usage:    wgpu::BufferUsages::UNIFORM,
                    });
                    let user_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label:    Some("offscreen-user-uniform"),
                        contents: user_bytes,
                        usage:    wgpu::BufferUsages::UNIFORM,
                    });
                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label:   Some("offscreen-quad-bg"),
                        layout:  &self.shader_bgl,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: quad_buf.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 1, resource: user_buf.as_entire_binding() },
                        ],
                    });
                    transient_quads.push((quad.pipeline, bind_group, scissor));
                }
                FrameItem::Glyphs { glyphs, clip } => {
                    transient_glyphs.push(
                        self.offscreen_glyph_transient(glyphs, *clip, width, height),
                    );
                }
                FrameItem::Image(q) => {
                    let scissor = match q.clip {
                        Some(c) => match scissor_for(c, width, height) {
                            Some(sc) => Some(sc),
                            None => { transient_images.push(None); continue; }
                        },
                        None => None,
                    };
                    transient_images.push(
                        self.image_draw_bind_group(q, width, height)
                            .map(|bg| (bg, scissor)),
                    );
                }
                FrameItem::Backdrop(_) => {
                    log::debug!("backdrop inside scroll content is not supported yet — skipped");
                    continue;
                }
                FrameItem::Offscreen(_) => {
                    debug_assert!(false, "nested offscreen items are not supported");
                    continue;
                }
            }
        }

        // A flush during the prep above invalidates glyph UVs built before
        // it — rebuild ONLY the glyph transients (nothing else references
        // the atlas). The rebuild cannot flush again: every needed slot
        // was allocated post-flush.
        if std::mem::take(&mut self.glyph_atlas.flushed) {
            transient_glyphs.clear();
            for item in items {
                if let FrameItem::Glyphs { glyphs, clip } = item {
                    transient_glyphs.push(
                        self.offscreen_glyph_transient(glyphs, *clip, width, height),
                    );
                }
            }
            self.glyph_atlas.flushed = false;
        }

        // Draw in item order. `layer_idx`/`quad_idx`/`glyph_idx` walk the
        // transient vecs in the same order they were filled above.
        let mut cleared = false;
        let mut layer_idx = 0usize;
        let mut quad_idx = 0usize;
        let mut glyph_idx = 0usize;
        let mut image_idx = 0usize;
        for item in items {
            let load = if !cleared {
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
            } else {
                wgpu::LoadOp::Load
            };
            match item {
                FrameItem::Pixels(layer) => {
                    if layer.width == 0 || layer.height == 0 { continue; }
                    let cached = &transient_layers[layer_idx];
                    layer_idx += 1;
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("offscreen-pixel-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           &target_view,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    rpass.set_pipeline(&self.pipeline_overlay);
                    rpass.set_bind_group(0, &cached.bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Shader(quad) => {
                    if quad.clip.is_some() && scissor_for(quad.clip.unwrap(), width, height).is_none() {
                        continue;
                    }
                    let (pipeline_id, bind_group, scissor) = &transient_quads[quad_idx];
                    quad_idx += 1;
                    let Some(entry) = self.shader_pipelines.get(pipeline_id) else {
                        if self.missing_pipeline_warned.insert(*pipeline_id) {
                            log::error!("offscreen quad references unregistered pipeline {pipeline_id}");
                        }
                        continue;
                    };
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("offscreen-quad-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           &target_view,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(*x, *y, *w, *h);
                    }
                    rpass.set_pipeline(&entry.pipeline);
                    rpass.set_bind_group(0, bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Glyphs { .. } => {
                    let prepared = &transient_glyphs[glyph_idx];
                    glyph_idx += 1;
                    let Some((bind_group, inst_buf, count, scissor)) = prepared else { continue; };
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("offscreen-glyph-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           &target_view,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(*x, *y, *w, *h);
                    }
                    rpass.set_pipeline(&self.glyph_pipeline);
                    rpass.set_bind_group(0, bind_group, &[]);
                    rpass.set_vertex_buffer(0, inst_buf.slice(..));
                    rpass.draw(0..6, 0..*count);
                }
                FrameItem::Image(_) => {
                    let prepared = &transient_images[image_idx];
                    image_idx += 1;
                    let Some((bind_group, scissor)) = prepared else { continue; };
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("offscreen-image-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           &target_view,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(*x, *y, *w, *h);
                    }
                    rpass.set_pipeline(&self.image_pipeline);
                    rpass.set_bind_group(0, bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Backdrop(_) => continue,
                FrameItem::Offscreen(_) => continue,
            }
        }
        // An all-empty item list still needs the clear (content removed).
        if !cleared {
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Register (or replace) a shader pipeline (D109). Compiles EAGERLY —
    /// right here, never lazily on first paint (the Impeller lesson,
    /// PHASE_27.md). `wgsl_fragment` is the fragment-stage source; the
    /// framework prepends the quad vertex stage and placement uniform (see
    /// `shader_quad_header.wgsl` for the authoring contract).
    ///
    /// Returns `false` (and registers nothing) when the WGSL fails
    /// validation — the error is logged with the pipeline id. Failing loudly
    /// at registration is the whole point of eager compilation.
    ///
    /// Takes primitives only (`u64`/`&str`/[`ShaderBlend`]) — this crate is
    /// Layer 0 and cannot import `rosace-shader`'s typed `ShaderSpec`;
    /// `rosace-platform` converts.
    pub fn register_shader(&mut self, pipeline: u64, wgsl_fragment: &str, blend: ShaderBlend) -> bool {
        let source = format!(
            "{}\n{}",
            include_str!("shader_quad_header.wgsl"),
            wgsl_fragment,
        );

        // Scope validation errors so a bad shader is a logged failure, not
        // a process-level panic from wgpu's uncaptured-error handler.
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);

        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("registered-shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let blend_state = match blend {
            ShaderBlend::Alpha    => Some(wgpu::BlendState::ALPHA_BLENDING),
            ShaderBlend::Opaque   => Some(wgpu::BlendState::REPLACE),
            ShaderBlend::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation:  wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation:  wgpu::BlendOperation::Add,
                },
            }),
        };

        let compiled = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("registered-shader-pipeline"),
            layout: Some(&self.shader_pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &module,
                entry_point:         Some("vs_main"),
                buffers:             &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format:     self.config.format,
                    blend:      blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        if let Some(err) = pollster::block_on(self.device.pop_error_scope()) {
            log::error!("shader pipeline {pipeline} failed to compile: {err}");
            return false;
        }

        // Replacement invalidates any cached quad bound to the old pipeline
        // object — drop quad caches so the next present rebuilds them.
        if self.shader_pipelines.insert(pipeline, ShaderPipelineEntry { pipeline: compiled }).is_some() {
            self.cached_quads.clear();
        }
        self.missing_pipeline_warned.remove(&pipeline);
        log::info!("shader pipeline {pipeline} registered ({} bytes of WGSL)", wgsl_fragment.len());
        true
    }

    /// Resize the wgpu surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        if (width, height) == (self.width, self.height) { return; }
        self.width  = width;
        self.height = height;
        self.config.width  = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        // A reconfigure invalidates two things the D089 skip/reuse caches
        // assume, and both produced REAL live-resize bugs (2026-07-12):
        //
        // 1. "The swapchain already shows this frame." It doesn't — a
        //    reconfigured surface's contents are undefined. If the next
        //    frame's content happens to be byte-identical (window grew at
        //    the bottom-right, content anchored top-left), every
        //    `*_unchanged` comparison passes and the present is SKIPPED,
        //    leaving garbage/blank on screen. Clearing the compare-state
        //    guarantees the first present after a resize always draws.
        //
        // 2. Quad placement uniforms. `shader_quad_uniform(rect, sw, sh)`
        //    bakes the SURFACE SIZE into each cached quad's px→NDC mapping,
        //    but `sync_cached_quads` only rewrites the buffer when the
        //    quad's RECT changes — so after a resize, any quad whose pixel
        //    rect is unchanged kept its old-surface mapping and rendered
        //    scaled/shifted, while the glyph pipeline (whose globals ARE
        //    keyed on surface size) rendered correctly: backgrounds and
        //    text visibly disagreeing during/after a window drag. Clearing
        //    forces every quad uniform to rebuild against the new size.
        //
        // (`cached_images` is compare-metadata only — the image TEXTURE
        // cache is untouched, so this costs a redraw, not re-uploads.
        // Offscreen scroll layers are safe without clearing: their placed
        // uniform is recomputed against the live surface size every draw.)
        self.cached_quads.clear();
        self.cached_layers.clear();
        self.cached_glyph_batches.clear();
        self.cached_images.clear();
        self.cached_backdrops.clear();
    }

    /// Present a single opaque layer (backward-compatible shim for Phase 15 API).
    pub fn present(&mut self, pixels: &[u8], pixel_width: u32, pixel_height: u32) {
        self.present_layers(&[CompositorLayer::opaque(pixels, pixel_width, pixel_height)]);
    }

    /// Composite and present one or more pixel layers (D076, D077, D079).
    ///
    /// Backward-compatible wrapper over [`Self::present_frame`] for callers
    /// with no shader quads.
    pub fn present_layers(&mut self, layers: &[CompositorLayer<'_>]) {
        let items: Vec<FrameItem<'_>> = layers
            .iter()
            .map(|l| FrameItem::Pixels(CompositorLayer {
                pixels:     l.pixels,
                width:      l.width,
                height:     l.height,
                dest:       l.dest,
                src_offset: l.src_offset,
                dirty:      l.dirty,
            }))
            .collect();
        self.present_frame(&items);
    }

    /// Composite and present a frame of ordered items (D076-D079, D089,
    /// D109): pixel layers and shader quads, drawn strictly in slice order,
    /// bottom-to-top.
    ///
    /// - Pixel layers keep the D089 persistent-texture cache: clean layers
    ///   upload nothing; the first item clears/overwrites the surface.
    /// - Shader quads run their registered pipeline directly on the surface
    ///   (uniform buffers persisted per slot, rewritten only on change).
    /// - When EVERY item is unchanged from the previous frame, the present
    ///   is skipped entirely — no upload, no encoder, no surface acquire
    ///   (frame-skip preservation, Phase 27 constraint C4). A time-animated
    ///   shader must therefore take its clock as a uniform: uniforms are
    ///   what dirtiness is measured by.
    ///
    /// Pass an empty slice to skip presentation for this frame.
    pub fn present_frame(&mut self, items: &[FrameItem<'_>]) {
        if items.is_empty() { return; }
        let (sw, sh) = (self.width, self.height);

        let pixel_layers: Vec<&CompositorLayer<'_>> = items.iter()
            .filter_map(|i| match i { FrameItem::Pixels(l) => Some(l), _ => None })
            .collect();
        let quads: Vec<&ShaderQuad<'_>> = items.iter()
            .filter_map(|i| match i { FrameItem::Shader(q) => Some(q), _ => None })
            .collect();
        let offs: Vec<&OffscreenRef> = items.iter()
            .filter_map(|i| match i { FrameItem::Offscreen(o) => Some(o), _ => None })
            .collect();
        #[allow(clippy::type_complexity)]
        let glyph_batches: Vec<(&Vec<AtlasGlyph<'_>>, Option<(f32, f32, f32, f32)>)> = items.iter()
            .filter_map(|i| match i {
                FrameItem::Glyphs { glyphs, clip } => Some((glyphs, *clip)),
                _ => None,
            })
            .collect();
        let images: Vec<&ImageQuad<'_>> = items.iter()
            .filter_map(|i| match i { FrameItem::Image(q) => Some(q), _ => None })
            .collect();
        let backdrops: Vec<BackdropQuad> = items.iter()
            .filter_map(|i| match i { FrameItem::Backdrop(b) => Some(*b), _ => None })
            .collect();
        if pixel_layers.is_empty() && quads.is_empty() && offs.is_empty()
            && glyph_batches.is_empty() && images.is_empty() && backdrops.is_empty() { return; }
        let backdrops_unchanged = self.cached_backdrops == backdrops;

        let new_images: Vec<ImageDrawData> =
            images.iter().map(|q| (q.key, q.dest, q.opacity, q.clip)).collect();
        let images_unchanged = self.cached_images == new_images;

        // Upload first-seen glyphs + build per-batch instance floats
        // (D109 Step 4). Runs before the skip check so the comparison
        // sees the final instance data.
        let new_glyph_batches: Vec<GlyphBatchData> = glyph_batches
            .iter()
            .map(|(glyphs, clip)| (*clip, self.prepare_glyphs(glyphs)))
            .collect();
        let glyphs_unchanged = self.cached_glyph_batches == new_glyph_batches;

        // ── Skip-present fast path (D089) ──────────────────────────────────
        // If the frame is structurally identical to the last one — same
        // items, no dirty pixels, no moved offsets, identical quad uniforms —
        // the composited image is byte-for-byte what the swapchain already
        // shows. Do nothing.
        let layers_unchanged = self.cached_layers.len() == pixel_layers.len()
            && pixel_layers.iter().enumerate().all(|(i, l)| {
                let c = &self.cached_layers[i];
                !l.dirty
                    && c.width == l.width
                    && c.height == l.height
                    && c.uniform == l.uniform(sw, sh)
            });
        let quads_unchanged = self.cached_quads.len() == quads.len()
            && quads.iter().enumerate().all(|(i, q)| {
                let c = &self.cached_quads[i];
                c.pipeline == q.pipeline
                    && c.rect == q.rect
                    && c.uniforms == q.uniforms
                    && c.clip == q.clip
            });
        let offscreen_unchanged = offs.iter().all(|o| {
            !o.dirty
                && self.offscreen.get(&o.key)
                    .map(|e| e.uniform == placed_uniform(
                        Some(o.dest), e.width, e.height, o.src_offset, sw, sh,
                    ))
                    .unwrap_or(false)
        });
        if layers_unchanged && quads_unchanged && offscreen_unchanged && glyphs_unchanged
            && images_unchanged && backdrops_unchanged
        {
            log::debug!(
                "compositor: skip present ({} layers + {} quads + {} offscreen + {} glyph batches + {} images unchanged)",
                pixel_layers.len(), quads.len(), offs.len(), glyph_batches.len(), images.len(),
            );
            return;
        }

        // Acquire the swapchain texture BEFORE committing any of the
        // `cached_*` skip-present bookkeeping below. During an OS-level
        // live-resize drag on macOS, `get_current_texture()` routinely
        // returns `Outdated` for a frame or two right after `resize()`
        // reconfigures the surface — the old code silently `return`ed much
        // further down, AFTER already committing this frame's
        // `cached_glyph_batches`/`cached_images`/`cached_backdrops` to
        // "this frame's" values. The frame was then never actually drawn,
        // but the D089 skip-present fast path above would then think it
        // HAD been (the caches already matched), so content could silently
        // stay missing for many frames after — until something else
        // changed. Bailing out here, before the caches below are touched,
        // means a dropped frame is just a dropped frame: the next
        // `present_frame` call sees the real diff again and draws it.
        let output = match self.surface.get_current_texture() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(o) => o,
                    Err(e) => {
                        log::debug!("compositor: surface unavailable after reconfigure ({e:?}), skipping frame");
                        return;
                    }
                }
            }
            Err(e) => {
                log::debug!("compositor: surface unavailable ({e:?}), skipping frame");
                return;
            }
        };

        // A mid-prepare atlas flush invalidates instance UVs built earlier
        // in this pass — rebuild from the now-stable slot map (every alloc
        // already happened, so a rebuild cannot flush again unless one
        // frame's working set exceeds the whole atlas).
        let new_glyph_batches = if std::mem::take(&mut self.glyph_atlas.flushed) {
            glyph_batches.iter()
                .map(|(glyphs, clip)| (*clip, self.prepare_glyphs(glyphs)))
                .collect()
        } else {
            new_glyph_batches
        };
        self.glyph_atlas.flushed = false;
        self.cached_glyph_batches = new_glyph_batches;
        self.cached_images = new_images;
        self.cached_backdrops = backdrops.clone();

        // LRU clock tick — image textures drawn by THIS present get this
        // stamp and are exempt from eviction below.
        self.present_seq += 1;

        // Per-draw image bind groups, in item order (uploads first-seen
        // textures as a side effect).
        let prepared_images: Vec<Option<wgpu::BindGroup>> = images
            .iter()
            .map(|q| self.image_draw_bind_group(q, sw, sh))
            .collect();

        // Glyph globals track the surface size (vertex px→NDC mapping).
        if self.glyph_globals_size != (sw, sh) {
            self.glyph_globals_size = (sw, sh);
            let globals = [sw as f32, sh as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
            self.queue.write_buffer(&self.glyph_globals_buf, 0, bytemuck_f32x8(&globals));
        }
        log::debug!(
            "compositor: present {} layers ({} dirty) + {} shader quads",
            pixel_layers.len(),
            pixel_layers.iter().filter(|l| l.dirty).count(),
            quads.len(),
        );

        // ── Sync the persistent texture cache to this frame's layers ───────
        // Reuse a slot's texture when dimensions match: dirty slots get a
        // `write_texture` (no realloc); clean slots are left untouched. Offset
        // changes are a cheap uniform-buffer write — clean pixels, moved layer
        // (e.g. scroll) uploads nothing.
        self.cached_layers.truncate(pixel_layers.len());
        for (idx, layer) in pixel_layers.iter().enumerate() {
            if layer.width == 0 || layer.height == 0 { continue; }

            let dims_match = self.cached_layers.get(idx)
                .map(|c| c.width == layer.width && c.height == layer.height)
                .unwrap_or(false);

            if dims_match {
                let cached = &mut self.cached_layers[idx];
                if layer.dirty {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture:   &cached.texture,
                            mip_level: 0,
                            origin:    wgpu::Origin3d::ZERO,
                            aspect:    wgpu::TextureAspect::All,
                        },
                        layer.pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset:         0,
                            bytes_per_row:  Some(layer.width * 4),
                            rows_per_image: Some(layer.height),
                        },
                        wgpu::Extent3d {
                            width:                 layer.width,
                            height:                layer.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                let uniform = layer.uniform(sw, sh);
                if cached.uniform != uniform {
                    self.queue.write_buffer(&cached.uniform_buf, 0, bytemuck_f32x8(&uniform));
                    cached.uniform = uniform;
                }
            } else {
                let cached = self.build_cached_layer(layer, sw, sh);
                if idx < self.cached_layers.len() {
                    self.cached_layers[idx] = cached;
                } else {
                    self.cached_layers.push(cached);
                }
            }
        }

        // ── Sync the persistent quad resources (same D089 discipline) ──────
        self.sync_cached_quads(&quads, sw, sh);

        // ── Composite the items, in order ───────────────────────────────────
        // Frames containing a Backdrop render into an intermediate scene
        // texture (so the glass can SAMPLE what's beneath it) and blit to
        // the surface at the end; all other frames render direct.
        let use_scene = !backdrops.is_empty();
        if use_scene { self.ensure_scene(sw, sh); }
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target: &wgpu::TextureView = if use_scene {
            &self.scene.as_ref().expect("ensure_scene above").scene_view
        } else {
            &view
        };

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor-enc"),
        });

        // The first executed pass clears the surface; every later pass loads
        // the already-rendered content.
        let mut cleared = false;
        let mut pixel_idx = 0usize;
        let mut quad_idx = 0usize;
        let mut glyph_idx = 0usize;
        let mut image_idx = 0usize;

        for item in items {
            match item {
                FrameItem::Pixels(layer) => {
                    let idx = pixel_idx;
                    pixel_idx += 1;
                    if layer.width == 0 || layer.height == 0 { continue; }
                    let Some(cached) = self.cached_layers.get(idx) else { continue; };

                    // REPLACE-blend base pipeline only for the very first
                    // pass of the frame (full overwrite); everything later
                    // alpha-blends over it.
                    let pipeline = if !cleared { &self.pipeline_base } else { &self.pipeline_overlay };
                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("compositor-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    rpass.set_pipeline(pipeline);
                    rpass.set_bind_group(0, &cached.bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Shader(quad) => {
                    let idx = quad_idx;
                    quad_idx += 1;
                    if quad.rect.2 <= 0.0 || quad.rect.3 <= 0.0 { continue; }

                    let Some(entry) = self.shader_pipelines.get(&quad.pipeline) else {
                        // Unregistered id: loud once, silent after — a
                        // per-frame error would flood at 120fps.
                        if self.missing_pipeline_warned.insert(quad.pipeline) {
                            log::error!(
                                "shader quad references unregistered pipeline {} — \
                                 was register_shader called before the first frame?",
                                quad.pipeline,
                            );
                        }
                        continue;
                    };
                    let Some(cached) = self.cached_quads.get(idx) else { continue; };

                    // Widget clip → hardware scissor. Empty intersection ⇒
                    // nothing to draw.
                    let scissor = match quad.clip {
                        Some(c) => match scissor_for(c, sw, sh) {
                            Some(s) => Some(s),
                            None => continue,
                        },
                        None => None,
                    };

                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("shader-quad-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(x, y, w, h);
                    }
                    rpass.set_pipeline(&entry.pipeline);
                    rpass.set_bind_group(0, &cached.bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Offscreen(o) => {
                    // Update the placed uniform (the scroll offset lives
                    // here) BEFORE the pass borrows the bind group.
                    let uniform_changed = {
                        let Some(entry) = self.offscreen.get_mut(&o.key) else {
                            log::debug!("offscreen {} referenced before render_offscreen", o.key);
                            continue;
                        };
                        let uniform = placed_uniform(
                            Some(o.dest), entry.width, entry.height, o.src_offset, sw, sh,
                        );
                        if entry.uniform != uniform {
                            entry.uniform = uniform;
                            true
                        } else {
                            false
                        }
                    };
                    let entry = &self.offscreen[&o.key];
                    if uniform_changed {
                        self.queue.write_buffer(&entry.uniform_buf, 0, bytemuck_f32x8(&entry.uniform));
                    }

                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("offscreen-sample-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    rpass.set_pipeline(&self.pipeline_overlay);
                    rpass.set_bind_group(0, &entry.bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Glyphs { .. } => {
                    let (clip, instances) = &self.cached_glyph_batches[glyph_idx];
                    glyph_idx += 1;
                    if instances.is_empty() { continue; }
                    let scissor = match clip {
                        Some(c) => match scissor_for(*c, sw, sh) {
                            Some(s) => Some(s),
                            None => continue,
                        },
                        None => None,
                    };
                    // Transient instance buffer — a text-heavy frame is a
                    // few KB; revisit with a persistent ring if measured.
                    let inst_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label:    Some("glyph-instances"),
                        contents: f32s_as_bytes(instances),
                        usage:    wgpu::BufferUsages::VERTEX,
                    });
                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("glyph-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(x, y, w, h);
                    }
                    rpass.set_pipeline(&self.glyph_pipeline);
                    rpass.set_bind_group(0, &self.glyph_atlas.bind_group, &[]);
                    rpass.set_vertex_buffer(0, inst_buf.slice(..));
                    rpass.draw(0..6, 0..(instances.len() / 12) as u32);
                }
                FrameItem::Backdrop(b) => {
                    let Some(st) = self.scene.as_ref() else { continue; };
                    let spread = (b.blur / 3.0).max(1.0);
                    let mk_blur_bg = |src: &wgpu::TextureView, texel: (f32, f32), dir: (f32, f32)| {
                        let u = [texel.0, texel.1, dir.0, dir.1, spread, 0.0, 0.0, 0.0];
                        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label:    Some("blur-uniform"),
                            contents: bytemuck_f32x8(&u),
                            usage:    wgpu::BufferUsages::UNIFORM,
                        });
                        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label:   Some("blur-bg"),
                            layout:  &self.image_bgl,
                            entries: &[
                                wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler_linear) },
                            ],
                        })
                    };
                    // H: scene -> half_a, V: half_a -> half_b.
                    let passes = [
                        (&st.scene_view,  &st.half_a_view, (1.0 / sw as f32, 1.0 / sh as f32), (1.0f32, 0.0f32)),
                        (&st.half_a_view, &st.half_b_view, (2.0 / sw as f32, 2.0 / sh as f32), (0.0, 1.0)),
                    ];
                    for (src, dst, texel, dir) in passes {
                        let bg = mk_blur_bg(src, texel, dir);
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("backdrop-blur-pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: dst,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes:         None,
                            occlusion_query_set:      None,
                        });
                        rpass.set_pipeline(&self.blur_pipeline);
                        rpass.set_bind_group(0, &bg, &[]);
                        rpass.draw(0..6, 0..1);
                    }
                    // Glass panel into the scene, sampling the blurred copy.
                    let ndc = shader_quad_uniform(b.rect, sw, sh);
                    let (x, y, w, h) = b.rect;
                    let gu: [f32; 16] = [
                        ndc[0], ndc[1], ndc[2], ndc[3],
                        x / sw as f32, y / sh as f32, w / sw as f32, h / sh as f32,
                        w, h, b.radius, 0.0,
                        b.tint[0], b.tint[1], b.tint[2], b.tint[3],
                    ];
                    let gbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label:    Some("glass-uniform"),
                        contents: f32s_as_bytes(&gu),
                        usage:    wgpu::BufferUsages::UNIFORM,
                    });
                    let gbg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label:   Some("glass-bg"),
                        layout:  &self.image_bgl,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&st.half_b_view) },
                            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler_linear) },
                        ],
                    });
                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("backdrop-glass-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    rpass.set_pipeline(&self.glass_pipeline);
                    rpass.set_bind_group(0, &gbg, &[]);
                    rpass.draw(0..6, 0..1);
                }
                FrameItem::Image(q) => {
                    let prepared = &prepared_images[image_idx];
                    image_idx += 1;
                    let Some(bind_group) = prepared else { continue; };
                    if q.dest.2 <= 0.0 || q.dest.3 <= 0.0 { continue; }
                    let scissor = match q.clip {
                        Some(c) => match scissor_for(c, sw, sh) {
                            Some(s) => Some(s),
                            None => continue,
                        },
                        None => None,
                    };
                    let load = if !cleared {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    };
                    cleared = true;
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("image-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view:           target,
                            resolve_target: None,
                            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes:         None,
                        occlusion_query_set:      None,
                    });
                    if let Some((x, y, w, h)) = scissor {
                        rpass.set_scissor_rect(x, y, w, h);
                    }
                    rpass.set_pipeline(&self.image_pipeline);
                    rpass.set_bind_group(0, bind_group, &[]);
                    rpass.draw(0..6, 0..1);
                }
            }
        }

        // Scene frames: blit the finished scene onto the surface.
        if use_scene {
            let st = self.scene.as_ref().expect("scene exists");
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            rpass.set_pipeline(&self.pipeline_base);
            rpass.set_bind_group(0, &st.blit_bind_group, &[]);
            rpass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        // Image-texture byte budget: evict least-recently-used OFF-SCREEN
        // entries until back under budget. On-screen entries (stamped with
        // this present's seq) are never evicted — if they alone exceed the
        // budget, warn once and carry on (dropping visible textures would
        // just thrash re-uploads every frame).
        if self.image_cache_bytes > IMAGE_CACHE_MAX_BYTES {
            let mut evictable: Vec<(u64, u64, usize)> = self.image_cache.iter()
                .filter(|(_, e)| e.last_used != self.present_seq)
                .map(|(k, e)| (e.last_used, *k, e.bytes))
                .collect();
            evictable.sort_unstable();
            for (_, key, bytes) in evictable {
                if self.image_cache_bytes <= IMAGE_CACHE_MAX_BYTES { break; }
                self.image_cache.remove(&key);
                self.image_cache_bytes -= bytes;
                log::debug!("image cache: evicted {key:#x} ({bytes} bytes)");
            }
            if self.image_cache_bytes > IMAGE_CACHE_MAX_BYTES && !self.image_budget_warned {
                self.image_budget_warned = true;
                log::warn!(
                    "on-screen images alone exceed the {}MB texture budget",
                    IMAGE_CACHE_MAX_BYTES / (1024 * 1024),
                );
            }
        }

        // Evict offscreen targets no longer referenced by any frame item —
        // a removed scroll view must not pin its content texture forever.
        if !self.offscreen.is_empty() {
            let referenced: std::collections::HashSet<u64> =
                offs.iter().map(|o| o.key).collect();
            self.offscreen.retain(|k, _| referenced.contains(k));
        }
    }

    /// Sync `cached_quads` to this frame's quads: reuse buffers when the
    /// uniform size matches (rewriting only changed bytes), rebuild the slot
    /// otherwise. Mirrors the pixel-layer texture cache's D089 discipline.
    fn sync_cached_quads(&mut self, quads: &[&ShaderQuad<'_>], sw: u32, sh: u32) {
        /// Uniform bindings cannot be zero-sized — a shader with no uniforms
        /// binds 16 zero bytes.
        const EMPTY_UNIFORMS: [u8; 16] = [0u8; 16];

        self.cached_quads.truncate(quads.len());
        for (idx, quad) in quads.iter().enumerate() {
            let user_bytes: &[u8] =
                if quad.uniforms.is_empty() { &EMPTY_UNIFORMS } else { quad.uniforms };
            let quad_uniform = shader_quad_uniform(quad.rect, sw, sh);

            let reusable = self.cached_quads.get(idx)
                .map(|c| c.user_len == user_bytes.len())
                .unwrap_or(false);

            if reusable {
                let cached = &mut self.cached_quads[idx];
                if cached.rect != quad.rect {
                    self.queue.write_buffer(&cached.quad_buf, 0, bytemuck_f32x8(&quad_uniform));
                }
                if cached.uniforms != quad.uniforms {
                    self.queue.write_buffer(&cached.user_buf, 0, user_bytes);
                }
                cached.pipeline = quad.pipeline;
                cached.rect     = quad.rect;
                cached.uniforms = quad.uniforms.to_vec();
                cached.clip     = quad.clip;
            } else {
                let quad_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label:    Some("shader-quad-uniform"),
                    contents: bytemuck_f32x8(&quad_uniform),
                    usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let user_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label:    Some("shader-user-uniform"),
                    contents: user_bytes,
                    usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label:   Some("shader-quad-bg"),
                    layout:  &self.shader_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: quad_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: user_buf.as_entire_binding() },
                    ],
                });
                let cached = CachedShaderQuad {
                    pipeline:   quad.pipeline,
                    rect:       quad.rect,
                    uniforms:   quad.uniforms.to_vec(),
                    clip:       quad.clip,
                    quad_buf,
                    user_buf,
                    user_len:   user_bytes.len(),
                    bind_group,
                };
                if idx < self.cached_quads.len() {
                    self.cached_quads[idx] = cached;
                } else {
                    self.cached_quads.push(cached);
                }
            }
        }
    }

    /// Create a persistent texture + bind group for a fresh layer slot and
    /// upload its initial pixels (D089).
    ///
    /// Format is `Rgba8UnormSrgb`, not `Rgba8Unorm` — the bytes `tiny-skia`
    /// produces are already gamma-encoded sRGB (standard 8-bit display
    /// bytes), and the swapchain surface is configured to an sRGB format
    /// too (`.find(|f| f.is_srgb())` above). An `*Srgb` texture format tells
    /// the GPU to sample-time-decode this texture's bytes to linear before
    /// the fragment shader sees them; the sRGB surface then re-encodes on
    /// write — one correct round-trip. Using plain `Unorm` here (the bug,
    /// fixed 2026-07-08) skipped the decode, so every already-encoded byte
    /// got sRGB-encoded a SECOND time on write to the surface — verified by
    /// sampling actual rendered pixels: a `#2B2D30` (43,45,48) theme surface
    /// color rendered as (96,98,102), a ~2.2x lightening concentrated in
    /// darks (exactly the sRGB curve's shape) and negligible near white —
    /// which is why this went unnoticed until a proper dark theme made the
    /// washed-out/low-contrast result obvious.
    fn build_cached_layer(&self, layer: &CompositorLayer<'_>, surface_w: u32, surface_h: u32) -> CachedLayer {
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label:           Some("frame-layer"),
                size:            wgpu::Extent3d {
                    width:                 layer.width,
                    height:                layer.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count:    1,
                dimension:       wgpu::TextureDimension::D2,
                format:          wgpu::TextureFormat::Rgba8UnormSrgb,
                usage:           wgpu::TextureUsages::TEXTURE_BINDING
                               | wgpu::TextureUsages::COPY_DST,
                view_formats:    &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            layer.pixels,
        );
        let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Write layer uniforms: dest NDC + UV window (D090). 8 floats = 32 bytes.
        let uniform = layer.uniform(surface_w, surface_h);
        let uniform_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("layer-uniforms"),
            contents: bytemuck_f32x8(&uniform),
            usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("layer-bg"),
            layout:  &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding:  2,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        });

        CachedLayer {
            texture,
            bind_group,
            uniform_buf,
            width:  layer.width,
            height: layer.height,
            uniform,
        }
    }

    /// Physical size of the configured surface.
    pub fn surface_size(&self) -> (u32, u32) { (self.width, self.height) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_quad_uniform_maps_full_surface_to_full_ndc() {
        let u = shader_quad_uniform((0.0, 0.0, 800.0, 600.0), 800, 600);
        assert_eq!(&u[0..4], &[-1.0, -1.0, 1.0, 1.0], "full-surface rect must span the whole clip space");
        assert_eq!(&u[4..6], &[800.0, 600.0], "size_px must be the rect size");
    }

    #[test]
    fn shader_quad_uniform_maps_a_centered_rect_symmetrically() {
        // A 400x300 rect centered on an 800x600 surface: NDC ±0.5.
        let u = shader_quad_uniform((200.0, 150.0, 400.0, 300.0), 800, 600);
        assert!((u[0] + 0.5).abs() < 1e-6, "left: {}", u[0]);
        assert!((u[1] + 0.5).abs() < 1e-6, "bottom: {}", u[1]);
        assert!((u[2] - 0.5).abs() < 1e-6, "right: {}", u[2]);
        assert!((u[3] - 0.5).abs() < 1e-6, "top: {}", u[3]);
    }

    #[test]
    fn shelf_packer_fills_shelves_then_exhausts_then_resets() {
        let mut p = ShelfPacker::new(64);
        // 20x20 glyphs (+1px gutter = 21): 3 per shelf row, 3 shelves = 9.
        let mut got = 0;
        while p.alloc(20, 20).is_some() { got += 1; }
        assert!(got >= 6, "should pack several 20x20 glyphs into 64x64, got {got}");
        assert!(p.alloc(20, 20).is_none(), "exhausted");
        // Oversized never fits.
        assert!(ShelfPacker::new(64).alloc(100, 10).is_none());
        // A fresh packer (the flush path) allocates again.
        assert!(ShelfPacker::new(64).alloc(20, 20).is_some());
    }

    #[test]
    fn shelf_packer_slots_never_overlap_or_escape_bounds() {
        let mut p = ShelfPacker::new(128);
        let mut taken: Vec<(u32, u32, u32, u32)> = Vec::new();
        for (w, h) in [(30u32, 12u32), (30, 12), (50, 30), (10, 10), (60, 12), (40, 28)] {
            if let Some((x, y)) = p.alloc(w, h) {
                assert!(x + w <= 128 && y + h <= 128, "escaped bounds: {x},{y} {w}x{h}");
                for &(tx, ty, tw, th) in &taken {
                    let overlap = x < tx + tw && tx < x + w && y < ty + th && ty < y + h;
                    assert!(!overlap, "overlap: ({x},{y},{w},{h}) vs ({tx},{ty},{tw},{th})");
                }
                taken.push((x, y, w, h));
            }
        }
        assert!(taken.len() >= 5, "most allocations should fit");
    }

    #[test]
    fn scissor_clamps_to_surface_and_rejects_empty() {
        // Clip hanging off the top-left is clamped.
        assert_eq!(scissor_for((-10.0, -10.0, 50.0, 50.0), 100, 100), Some((0, 0, 40, 40)));
        // Clip hanging off the bottom-right is clamped.
        assert_eq!(scissor_for((80.0, 90.0, 50.0, 50.0), 100, 100), Some((80, 90, 20, 10)));
        // Fully outside → None (draw nothing).
        assert_eq!(scissor_for((200.0, 200.0, 50.0, 50.0), 100, 100), None);
        // Zero-area (the canvas's degenerate empty-intersection clip) → None.
        assert_eq!(scissor_for((30.0, 30.0, 0.0, 0.0), 100, 100), None);
    }
}
