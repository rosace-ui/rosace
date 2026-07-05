//! wgpu GPU compositor for TEZZERA (D072–D079).
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
                label:             Some("tezzera-compositor"),
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
        })
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

    /// Composite and present one or more layers (D076, D077, D079).
    ///
    /// Layers are blended bottom-to-top:
    /// - Layer 0: REPLACE blend (base, fully overwrites surface)
    /// - Layer 1+: ALPHA_BLENDING (Porter-Duff over on top of previous)
    ///
    /// Each layer's `opacity` scales its alpha channel before blending.
    /// Pass an empty slice to skip presentation for this frame.
    pub fn present_layers(&mut self, layers: &[CompositorLayer<'_>]) {
        if layers.is_empty() { return; }

        // ── Skip-present fast path (D089) ──────────────────────────────────
        // If the layer set is structurally identical to last frame — same
        // count, same dimensions, no dirty pixels, no moved offsets — then the
        // composited image is byte-for-byte what the swapchain already shows.
        // Do nothing: no upload, no encoder, no surface acquire. This is the
        // common case for hover/idle frames the frame-skip already made
        // raster-free upstream.
        let (sw, sh) = (self.width, self.height);
        let unchanged = self.cached_layers.len() == layers.len()
            && layers.iter().enumerate().all(|(i, l)| {
                let c = &self.cached_layers[i];
                !l.dirty
                    && c.width == l.width
                    && c.height == l.height
                    && c.uniform == l.uniform(sw, sh)
            });
        if unchanged {
            log::debug!("compositor: skip present ({} layers unchanged)", layers.len());
            return;
        }
        log::debug!(
            "compositor: present {} layers ({} dirty)",
            layers.len(),
            layers.iter().filter(|l| l.dirty).count(),
        );

        // ── Sync the persistent texture cache to this frame's layers ───────
        // Reuse a slot's texture when dimensions match: dirty slots get a
        // `write_texture` (no realloc); clean slots are left untouched. Offset
        // changes are a cheap uniform-buffer write — clean pixels, moved layer
        // (e.g. scroll) uploads nothing.
        self.cached_layers.truncate(layers.len());
        for (idx, layer) in layers.iter().enumerate() {
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

        // ── Composite the cached layers onto the surface ───────────────────
        let Ok(output) = self.surface.get_current_texture() else { return; };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor-enc"),
        });

        for (idx, layer) in layers.iter().enumerate() {
            if layer.width == 0 || layer.height == 0 { continue; }
            let Some(cached) = self.cached_layers.get(idx) else { continue; };

            let pipeline = if idx == 0 {
                &self.pipeline_base
            } else {
                &self.pipeline_overlay
            };

            // load: Clear only on the first pass; subsequent passes load the
            // already-rendered content so previous layers are preserved.
            let load = if idx == 0 {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                wgpu::LoadOp::Load
            };

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

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    /// Create a persistent texture + bind group for a fresh layer slot and
    /// upload its initial pixels (D089).
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
                format:          wgpu::TextureFormat::Rgba8Unorm,
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
