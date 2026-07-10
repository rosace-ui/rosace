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

/// One item of a presented frame, drawn strictly in slice order —
/// bottom-to-top z. Pixel layers keep their D089 persistent-texture cache;
/// shader quads execute their registered pipeline directly on the surface.
pub enum FrameItem<'a> {
    Pixels(CompositorLayer<'a>),
    Shader(ShaderQuad<'a>),
}

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
        let (sw, sh) = (surface_w.max(1) as f32, surface_h.max(1) as f32);
        let (tw, th) = (self.width.max(1) as f32, self.height.max(1) as f32);

        // Screen placement → NDC. Full surface when dest is None.
        let d = self.dest.unwrap_or(LayerRect { x: 0.0, y: 0.0, w: sw, h: sh });
        let ndc_left   = 2.0 * d.x / sw - 1.0;
        let ndc_right  = 2.0 * (d.x + d.w) / sw - 1.0;
        let ndc_top    = 1.0 - 2.0 * d.y / sh;
        let ndc_bottom = 1.0 - 2.0 * (d.y + d.h) / sh;

        // UV window: sample a d.w × d.h region of the texture starting at the
        // scroll offset (1:1 physical px → texel, no scaling).
        let uv_min_x  = self.src_offset.0 / tw;
        let uv_min_y  = self.src_offset.1 / th;
        let uv_span_x = d.w / tw;
        let uv_span_y = d.h / th;

        [
            ndc_left, ndc_bottom,   // dest_min
            ndc_right, ndc_top,     // dest_max
            uv_min_x, uv_min_y,     // uv_min
            uv_span_x, uv_span_y,   // uv_span
        ]
    }
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
                ..Default::default()
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
            missing_pipeline_warned: std::collections::HashSet::new(),
        })
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
        self.width  = width;
        self.height = height;
        self.config.width  = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
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
        if pixel_layers.is_empty() && quads.is_empty() { return; }

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
        if layers_unchanged && quads_unchanged {
            log::debug!(
                "compositor: skip present ({} layers + {} quads unchanged)",
                pixel_layers.len(), quads.len(),
            );
            return;
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

        // ── Composite the items onto the surface, in order ─────────────────
        let Ok(output) = self.surface.get_current_texture() else { return; };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor-enc"),
        });

        // The first executed pass clears the surface; every later pass loads
        // the already-rendered content.
        let mut cleared = false;
        let mut pixel_idx = 0usize;
        let mut quad_idx = 0usize;

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
                            view:           &view,
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
                            view:           &view,
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
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
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
