use std::num::NonZeroU32;
use std::sync::Arc;
use web_time::Instant;

#[cfg(debug_assertions)]
use rosace_trace::{event::RosaceTrace, trace};


use rosace_render::canvas::SkiaCanvas;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window as WinitWindow, WindowAttributes, WindowId};

use crate::event::{InputEvent, Key, MouseButton};

/// Sent to the winit event loop from any thread to wake it from `Wait` sleep.
///
/// `Atom::set()` calls `rosace_state::request_frame()`, which invokes the
/// registered wakeup closure, which sends this event. The event loop then
/// calls `window.request_redraw()` in the `user_event` handler.
pub struct FrameRequest;

pub struct PlatformWindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// Low-level windowed event loop. Accepts a raw canvas-paint closure.
/// For widget-based apps, use `rosace::App` from the umbrella crate instead.
pub struct PlatformWindow {
    config: PlatformWindowConfig,
}

impl PlatformWindow {
    pub fn new() -> Self {
        Self {
            config: PlatformWindowConfig {
                title: "Rosace".to_string(),
                width: 800,
                height: 600,
            },
        }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.config.title = t.into();
        self
    }

    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.config.width = w;
        self.config.height = h;
        self
    }

    /// Run with a single canvas (backward-compatible).
    ///
    /// Calls the closure with the base canvas only. The overlay canvas is
    /// always transparent. Internally uses `run_layered` with an adapter.
    pub fn run<F>(self, mut paint_fn: F)
    where
        F: FnMut(&mut SkiaCanvas, &[InputEvent]) + 'static,
    {
        self.run_layered(move |base, _overlay, events| paint_fn(base, events));
    }

    /// Run with two canvases: base layer and overlay layer (D076, Phase 16).
    ///
    /// The platform clears the overlay canvas to transparent before each call.
    /// Both canvases are uploaded as separate GPU textures and alpha-blended
    /// on the GPU (base first, overlay on top with `ALPHA_BLENDING`).
    pub fn run_layered<F>(self, paint_fn: F)
    where
        // `'static` so the app can be handed to the browser's rAF loop on web
        // (`spawn_app`); native `move` closures already satisfy it.
        F: FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent]) + 'static,
    {
        // Surface real panic messages in the browser console instead of a bare
        // "RuntimeError: unreachable".
        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();

        let event_loop = EventLoop::<FrameRequest>::with_user_event()
            .build()
            .expect("failed to create event loop");
        event_loop.set_control_flow(ControlFlow::Wait);

        // Register the wakeup fn BEFORE the first frame so background threads
        // (e.g. animation timers) can trigger redraws immediately.
        let proxy = event_loop.create_proxy();
        rosace_state::register_wakeup(move || {
            let _ = proxy.send_event(FrameRequest);
        });

        // Request the first frame immediately so the window paints on open.
        rosace_state::request_frame();

        let w = self.config.width;
        let h = self.config.height;
        let mut app = AppState {
            config: self.config,
            paint_fn,
            window: None,
            surface: None,
            context: None,
            presenter: None,
            canvas: SkiaCanvas::new(w, h),
            overlay_canvas: SkiaCanvas::new(w, h),
            pending_events: Vec::new(),
            frame_counter: 0,
            cursor_x: 0.0,
            cursor_y: 0.0,
            mouse_down: false,
            last_frame_time: None,
            scroll_layers: Vec::new(),
            shader_quads: Vec::new(),
            frame_items: Vec::new(),
            shader_fallback_warned: false,
        };
        // Native blocks on the OS event loop; web cannot block, so it hands the
        // app to the browser's requestAnimationFrame loop and returns.
        #[cfg(not(target_arch = "wasm32"))]
        event_loop.run_app(&mut app).unwrap();
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            event_loop.spawn_app(app);
        }
    }
}

impl Default for PlatformWindow {
    fn default() -> Self {
        Self::new()
    }
}

struct AppState<F> {
    config: PlatformWindowConfig,
    paint_fn: F,
    window: Option<Arc<WinitWindow>>,
    context: Option<softbuffer::Context<Arc<WinitWindow>>>,
    surface: Option<softbuffer::Surface<Arc<WinitWindow>, Arc<WinitWindow>>>,
    // GPU compositor (D072–D075). None → softbuffer fallback path is used.
    presenter: Option<rosace_compositor::GpuPresenter>,
    canvas: SkiaCanvas,
    // Overlay layer canvas — cleared to transparent each frame (D078).
    overlay_canvas: SkiaCanvas,
    pending_events: Vec<InputEvent>,
    frame_counter: u64,
    cursor_x: f32,
    cursor_y: f32,
    // True while a mouse button is held — CursorMoved requests frames only
    // then, so drags stream without paying for idle mouse movement.
    mouse_down: bool,
    last_frame_time: Option<Instant>,
    // Retained scroll layers (D090) — refreshed when the frame loop publishes,
    // reused across clean frames so they persist without a re-upload.
    scroll_layers: Vec<crate::scroll_layer::ScrollLayer>,
    // Retained GPU shader quads (D109) — refreshed on painted frames (the
    // canvas re-collects them on every `play_picture`), reused across clean
    // frames so quads persist through frame-skip like scroll layers do.
    shader_quads: Vec<rosace_render::ShaderQuadCmd>,
    // Retained ordered frame items for GPU-shapes mode (D109 Step 3 / C1):
    // the base canvas's quads + CPU segments, refreshed on painted frames,
    // reused across clean frames (same contract as shader_quads above).
    frame_items: Vec<rosace_render::canvas::CanvasFrameItem>,
    // Warn-once flag: shader registrations/quads on the softbuffer fallback
    // path (no GPU) are dropped — loud the first time, silent after.
    shader_fallback_warned: bool,
}

/// Drain queued `rosace-shader` registrations into the presenter's registry
/// (D109) — eager compilation at the frame boundary, converting the typed
/// `ShaderSpec` to the compositor's primitives-only API (its Layer-0
/// zero-rosace-deps contract means it cannot see `rosace-shader` types).
fn drain_shader_registrations(presenter: &mut rosace_compositor::GpuPresenter) {
    for (id, spec) in rosace_shader::take_pending_shaders() {
        let blend = match spec.blend {
            rosace_shader::BlendMode::Alpha    => rosace_compositor::ShaderBlend::Alpha,
            rosace_shader::BlendMode::Opaque   => rosace_compositor::ShaderBlend::Opaque,
            rosace_shader::BlendMode::Additive => rosace_compositor::ShaderBlend::Additive,
        };
        // Failure is already logged loudly by register_shader; nothing to
        // add here — the pipeline simply isn't registered and any quad
        // referencing it warns once at present time.
        let _ = presenter.register_shader(id.raw(), &spec.wgsl_source, blend);
    }
}

impl<F: FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent])> ApplicationHandler<FrameRequest> for AppState<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        // On iOS, the native UIWindow's frame/bounds — what actually
        // determines how large our rendered buffer appears on screen — is set
        // from winit's OWN internal screen-geometry computation at window
        // creation, which is unreliable on at least this winit/iOS-simulator
        // combination (verified: `outer_size()`/`inner_size()` reported
        // 1260x2280 physical vs the true, independently-confirmed 1179x2556 —
        // see `physical_canvas_size`). Sizing our own canvas/GPU surface
        // correctly (below) does NOT fix this: the OS still stretches that
        // buffer to fill whatever (wrong) frame the UIWindow already has,
        // which is what produced both the blurry/stretched look and the
        // right-edge clipping. `set_fullscreen(Borderless(None))` makes winit
        // call `UIWindow.setFrame(UIScreen.bounds)` internally (its exact
        // fullscreen-transition path — see winit's ios/window.rs
        // `set_fullscreen`) — the ONLY public API that corrects the frame to
        // the real screen bounds, so we drive it explicitly rather than trust
        // whatever frame the window started with.
        #[cfg(target_os = "ios")]
        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));

        #[cfg(target_os = "ios")]
        sync_ios_safe_area(&window);

        // On web, winit creates a <canvas> but (a) does not attach it to the
        // page and (b) ignores `with_inner_size` for it — the canvas keeps the
        // HTML default of 300x150, which crams the whole UI into a tiny box
        // (widgets overflow off-screen, click coords mismatch). Append it and
        // size it to fill the viewport: backing buffer in physical px, CSS box
        // in logical px. winit reads the canvas size back for `inner_size()`.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            if let Some(canvas) = window.canvas() {
                if let Some(web_win) = web_sys::window() {
                    let dpr = web_win.device_pixel_ratio();
                    let vw = web_win.inner_width().ok()
                        .and_then(|v| v.as_f64()).unwrap_or(800.0);
                    let vh = web_win.inner_height().ok()
                        .and_then(|v| v.as_f64()).unwrap_or(600.0);
                    canvas.set_width((vw * dpr) as u32);
                    canvas.set_height((vh * dpr) as u32);
                    let style = canvas.style();
                    let _ = style.set_property("width", &format!("{}px", vw));
                    let _ = style.set_property("height", &format!("{}px", vh));
                    let _ = style.set_property("display", "block");
                    web_win.document()
                        .and_then(|d| d.body())
                        .and_then(|b| b.append_child(&canvas).ok());
                }
            }
        }

        // Try GPU compositor (D072). Fall back to softbuffer if unavailable.
        // On web, wgpu init is async and `GpuPresenter::new` blocks on it
        // (`pollster::block_on`) — blocking is illegal on wasm and traps
        // ("RuntimeError: unreachable"), so use the CPU softbuffer path there.
        #[cfg(target_arch = "wasm32")]
        let presenter: Option<rosace_compositor::GpuPresenter> = None;
        #[cfg(not(target_arch = "wasm32"))]
        let presenter = rosace_compositor::GpuPresenter::new(
            window.clone(),
            self.config.width,
            self.config.height,
        );
        if presenter.is_some() {
            log::info!("rosace-platform: using GPU compositor (wgpu)");
        } else {
            // No GPU: nothing will ever compile shader pipelines for this
            // window. Registrations queued before startup are dropped now,
            // loudly, instead of accumulating forever.
            let dropped = rosace_shader::take_pending_shaders();
            if !dropped.is_empty() {
                log::warn!(
                    "rosace-platform: GPU unavailable — {} shader pipeline registration(s) \
                     dropped; DrawCommand::ShaderFill content will not render on the \
                     softbuffer fallback path",
                    dropped.len(),
                );
            }
            log::info!("rosace-platform: GPU compositor unavailable, using softbuffer");
            let context = softbuffer::Context::new(window.clone()).unwrap();
            let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
            self.context = Some(context);
            self.surface = Some(surface);
        }
        self.presenter = presenter;
        if let Some(p) = self.presenter.as_mut() {
            // GPU-shapes mode (D109/Phase 27 Step 3): built-in shape
            // commands render as SDF pipelines on the BASE canvas only —
            // scroll-content and overlay canvases stay tiny-skia until C2.
            // `ROSACE_CPU_SHAPES=1` is the kill switch (and the A/B
            // measurement lever): full tiny-skia path, as before Step 3.
            if std::env::var_os("ROSACE_CPU_SHAPES").is_none() {
                rosace_shader::builtin::register_builtins();
                self.canvas.set_gpu_shapes(true);
                log::info!("rosace-platform: GPU shapes enabled (ROSACE_CPU_SHAPES=1 to disable)");
            }
            // Eager pipeline compilation (D109, the Impeller lesson):
            // everything queued before `App::run` — including the
            // built-ins just registered — compiles right here at startup,
            // never lazily on first paint.
            drain_shader_registrations(p);
        }
        self.window = Some(window);
    }

    /// Wake from `ControlFlow::Wait` when an atom changes on any thread.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: FrameRequest) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                let scale = window.scale_factor() as f32;
                let phys = physical_canvas_size(window);
                let phys_w = phys.width;
                let phys_h = phys.height;
                if phys_w == 0 || phys_h == 0 {
                    return;
                }

                if let Some(surface) = self.surface.as_mut() {
                    surface
                        .resize(
                            NonZeroU32::new(phys_w).unwrap(),
                            NonZeroU32::new(phys_h).unwrap(),
                        )
                        .unwrap();
                }

                // Keep the GPU surface at the PHYSICAL canvas resolution every
                // frame. The presenter is initialised at the window's logical
                // size, so without this the first frame(s) render a physical
                // (Retina) canvas into a half-resolution surface and the OS
                // upscales the result → blurry text until a Resized event
                // happens to correct it. Syncing here guarantees a 1:1 map.
                // A surface reconfigure discards its contents, so force a
                // repaint+present this frame (never skip it via D089).
                if let Some(presenter) = self.presenter.as_mut() {
                    if presenter.surface_size() != (phys_w, phys_h) {
                        presenter.resize(phys_w, phys_h);
                        self.canvas.mark_frame_dirty();
                    }
                }

                let now = Instant::now();
                let dt = self.last_frame_time
                    .map(|t| t.elapsed().as_secs_f32())
                    .unwrap_or(1.0 / 60.0)
                    .clamp(0.001, 0.1);
                rosace_animate::set_frame_dt(dt);
                self.last_frame_time = Some(now);

                #[cfg(debug_assertions)]
                let frame = self.frame_counter;
                self.frame_counter += 1;

                #[cfg(debug_assertions)]
                trace!(RosaceTrace::FrameStart {
                    frame,
                    timestamp: now,
                });

                // Resize base + overlay canvases to match physical window size.
                if self.canvas.width() != phys_w
                    || self.canvas.height() != phys_h
                    || (self.canvas.scale() - scale).abs() > 0.01
                {
                    // Recreation must carry the GPU-shapes flag over — a
                    // resized window silently dropping to CPU shapes would
                    // be an invisible mode flip (D109).
                    let gpu_shapes = self.canvas.gpu_shapes();
                    self.canvas         = SkiaCanvas::new_hidpi(phys_w, phys_h, scale);
                    self.canvas.set_gpu_shapes(gpu_shapes);
                    self.overlay_canvas = SkiaCanvas::new_hidpi(phys_w, phys_h, scale);
                }

                // Clear overlay to transparent before each frame (D078).
                self.overlay_canvas.clear_transparent();

                let events = std::mem::take(&mut self.pending_events);
                (self.paint_fn)(&mut self.canvas, &mut self.overlay_canvas, &events);

                // Present the frame — GPU multi-layer compositor (D076, D079),
                // with softbuffer fallback that CPU-composites overlay on top.
                if let Some(presenter) = &mut self.presenter {
                    // Runtime shader registrations (D109) — anything queued
                    // since startup compiles NOW, before this frame's present
                    // could reference it. Startup registrations already
                    // compiled in `resumed`.
                    drain_shader_registrations(presenter);

                    // Per-frame dirtiness drives the compositor's texture cache
                    // (D089): a clean base layer reuses its persistent GPU
                    // texture, and a frame where nothing changed skips the
                    // present entirely. `take_frame_dirty` must run every frame
                    // so the flag resets; the base only repaints (and re-marks)
                    // when the frame loop actually redraws it.
                    let base_dirty = self.canvas.take_frame_dirty();

                    // Refresh the retained shader quads / frame items only on
                    // painted frames (`play_picture` re-collects the full set
                    // each paint — including painting to an empty set when
                    // shader content disappeared). Clean frames keep the
                    // retained set, same pattern as scroll layers below (D109).
                    if base_dirty {
                        if self.canvas.gpu_shapes() {
                            self.frame_items = self.canvas.take_frame_items();
                        } else {
                            self.shader_quads = self.canvas.take_shader_quads();
                        }
                    }
                    // Overlay shader content is not supported yet (the overlay
                    // is replayed every frame; quads there would need their own
                    // altitude in the item order) — drain so they can't
                    // accumulate, loud once if anything shows up.
                    let overlay_quads = self.overlay_canvas.take_shader_quads();
                    if !overlay_quads.is_empty() && !self.shader_fallback_warned {
                        self.shader_fallback_warned = true;
                        log::warn!(
                            "rosace-platform: {} ShaderFill command(s) recorded in the \
                             OVERLAY pass are not supported yet and were dropped",
                            overlay_quads.len(),
                        );
                    }

                    // Refresh the retained scroll layers only when the frame
                    // loop published (it repainted). `None` = clean frame →
                    // keep the retained set so the layers persist unchanged.
                    let refreshed = crate::scroll_layer::take_scroll_layers();
                    let scroll_dirty = refreshed.is_some();
                    if let Some(layers) = refreshed {
                        self.scroll_layers = layers;
                    }

                    // Composite bottom-to-top: base, shader quads (base-content
                    // altitude, D109 Step 2 — full per-command interleaving is
                    // Phase 27 C1), scroll layers (each placed at its
                    // viewport), then the overlay on top (D090). Scroll layers
                    // re-upload only on a publish frame (scroll_dirty);
                    // otherwise D089 reuses their persistent textures.
                    let mut items: Vec<rosace_compositor::FrameItem<'_>> = Vec::new();
                    if self.canvas.gpu_shapes() {
                        // GPU-shapes mode (D109 Step 3, C1): the frame IS
                        // the ordered item list — background quad, shape
                        // quads, and CPU segments (text/blits) placed at
                        // their bboxes, in command order. No full-frame
                        // base buffer exists.
                        for it in &self.frame_items {
                            match it {
                                rosace_render::canvas::CanvasFrameItem::Shader(q) => {
                                    items.push(rosace_compositor::FrameItem::Shader(
                                        rosace_compositor::ShaderQuad {
                                            pipeline: q.pipeline_id,
                                            rect:     q.rect,
                                            uniforms: &q.uniforms,
                                            clip:     q.clip,
                                        },
                                    ));
                                }
                                rosace_render::canvas::CanvasFrameItem::Segment { x, y, w, h, pixels } => {
                                    items.push(rosace_compositor::FrameItem::Pixels(
                                        rosace_compositor::CompositorLayer::placed(
                                            pixels, *w, *h,
                                            rosace_compositor::LayerRect {
                                                x: *x as f32, y: *y as f32,
                                                w: *w as f32, h: *h as f32,
                                            },
                                            (0.0, 0.0),
                                            base_dirty,
                                        ),
                                    ));
                                }
                            }
                        }
                    } else {
                        items.push(rosace_compositor::FrameItem::Pixels(
                            rosace_compositor::CompositorLayer::tracked(
                                self.canvas.pixels(), phys_w, phys_h, base_dirty,
                            ),
                        ));
                        for q in &self.shader_quads {
                            items.push(rosace_compositor::FrameItem::Shader(
                                rosace_compositor::ShaderQuad {
                                    pipeline: q.pipeline_id,
                                    rect:     q.rect,
                                    uniforms: &q.uniforms,
                                    clip:     q.clip,
                                },
                            ));
                        }
                    }
                    for sl in &self.scroll_layers {
                        // Live scroll offset from the non-reactive channel
                        // (physical px). A wheel tick updates this without a
                        // repaint, so a scroll-only frame is a uniform write
                        // over the reused content texture (D090).
                        let off = rosace_state::scroll_offset(sl.id);
                        items.push(rosace_compositor::FrameItem::Pixels(
                            rosace_compositor::CompositorLayer::placed(
                                &sl.pixels, sl.width, sl.height,
                                rosace_compositor::LayerRect {
                                    x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3,
                                },
                                (off[0] * scale, off[1] * scale),
                                scroll_dirty,
                            ),
                        ));
                    }
                    // Skip the overlay layer entirely when nothing drew into it
                    // this frame. When it did draw, treat it as dirty — the
                    // overlay is cleared and replayed every frame, so its pixels
                    // may differ even when the base is clean.
                    if self.overlay_canvas.has_drawn() {
                        items.push(rosace_compositor::FrameItem::Pixels(
                            rosace_compositor::CompositorLayer::tracked(
                                self.overlay_canvas.pixels(), phys_w, phys_h, true,
                            ),
                        ));
                    }
                    presenter.present_frame(&items);
                } else if let Some(surface) = &mut self.surface {
                    // Softbuffer fallback: no GPU, so ShaderFill content can't
                    // render. Drain quads so they don't accumulate; warn once.
                    let dropped = self.canvas.take_shader_quads();
                    let dropped_overlay = self.overlay_canvas.take_shader_quads();
                    if (!dropped.is_empty() || !dropped_overlay.is_empty())
                        && !self.shader_fallback_warned
                    {
                        self.shader_fallback_warned = true;
                        log::warn!(
                            "rosace-platform: DrawCommand::ShaderFill content dropped — \
                             GPU compositor unavailable (softbuffer fallback)",
                        );
                    }
                    let base_pixels = self.canvas.pixels();
                    let mut buffer = surface.buffer_mut().unwrap();

                    if self.overlay_canvas.has_drawn() {
                        // Overlay has content — Porter-Duff "over" blend.
                        let overlay_pixels = self.overlay_canvas.pixels();
                        for (i, pixel) in buffer.iter_mut().enumerate() {
                            let bi = i * 4;
                            let br  = base_pixels[bi]     as u32;
                            let bg  = base_pixels[bi + 1] as u32;
                            let bb  = base_pixels[bi + 2] as u32;
                            let oa  = overlay_pixels[bi + 3] as u32;
                            let or_ = overlay_pixels[bi]     as u32;
                            let og  = overlay_pixels[bi + 1] as u32;
                            let ob  = overlay_pixels[bi + 2] as u32;
                            let inv = 255 - oa;
                            let r = (or_ * oa + br * inv) / 255;
                            let g = (og  * oa + bg * inv) / 255;
                            let b = (ob  * oa + bb * inv) / 255;
                            *pixel = (r << 16) | (g << 8) | b;
                        }
                    } else {
                        // No overlay — fast path: copy base pixels directly,
                        // avoiding O(pixels) multiply/divide every frame.
                        for (i, pixel) in buffer.iter_mut().enumerate() {
                            let bi = i * 4;
                            let r = base_pixels[bi]     as u32;
                            let g = base_pixels[bi + 1] as u32;
                            let b = base_pixels[bi + 2] as u32;
                            *pixel = (r << 16) | (g << 8) | b;
                        }
                    }
                    buffer.present().unwrap();
                }

                #[cfg(debug_assertions)]
                {
                    let duration = now.elapsed();
                    let dropped = duration.as_secs_f64() * 1000.0 > 16.667;
                    trace!(RosaceTrace::FrameEnd {
                        frame,
                        duration,
                        dropped,
                    });
                }
            }

            WindowEvent::Resized(size) => {
                // On iOS, winit's event payload is the safe-area-reduced size;
                // re-query the true full-screen size so the canvas is never
                // smaller than the screen (see `physical_canvas_size`) — the
                // safe area is applied purely as Scaffold padding, not by
                // shrinking the canvas, so there is exactly one source of
                // truth for the inset instead of two disagreeing ones.
                let phys = self.window.as_ref().map(|w| physical_canvas_size(w)).unwrap_or(size);
                if let Some(presenter) = &mut self.presenter {
                    presenter.resize(phys.width, phys.height);
                }
                #[cfg(target_os = "ios")]
                if let Some(w) = &self.window {
                    sync_ios_safe_area(w); // orientation change moves the notch
                }
                self.pending_events.push(InputEvent::WindowResized {
                    width: phys.width,
                    height: phys.height,
                });
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // position is in physical pixels; convert to logical so hit
                // coordinates match the logical-pixel layout space.
                let scale = self.window.as_ref()
                    .map(|w| w.scale_factor())
                    .unwrap_or(1.0);
                self.cursor_x = (position.x / scale) as f32;
                self.cursor_y = (position.y / scale) as f32;
                self.pending_events.push(InputEvent::MouseMove {
                    x: self.cursor_x,
                    y: self.cursor_y,
                });
                // Request a frame on every move: hover tracking needs it,
                // and unchanged-hover frames are skipped cheaply (no raster).
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let (x, y) = (self.cursor_x, self.cursor_y);
                self.mouse_down = matches!(state, ElementState::Pressed);
                let ev = match state {
                    ElementState::Pressed  => InputEvent::MouseDown { x, y, button: btn },
                    ElementState::Released => InputEvent::MouseUp   { x, y, button: btn },
                };
                self.pending_events.push(ev);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::Touch(touch) => {
                // iOS and Android deliver touches, not mouse events. Map the
                // touch to the same pipeline as the mouse so taps, drags, and
                // scroll gestures work with the identical widget code. (Multi-
                // touch: every finger drives the single pointer — fine for now.)
                let scale = self.window.as_ref()
                    .map(|w| w.scale_factor())
                    .unwrap_or(1.0);
                self.cursor_x = (touch.location.x / scale) as f32;
                self.cursor_y = (touch.location.y / scale) as f32;
                let (x, y) = (self.cursor_x, self.cursor_y);
                match touch.phase {
                    winit::event::TouchPhase::Started => {
                        self.mouse_down = true;
                        // Position the pointer, then press — a fresh touch has
                        // no prior CursorMoved to set the hit location.
                        self.pending_events.push(InputEvent::MouseMove { x, y });
                        self.pending_events.push(InputEvent::MouseDown {
                            x, y, button: MouseButton::Left,
                        });
                    }
                    winit::event::TouchPhase::Moved => {
                        self.pending_events.push(InputEvent::MouseMove { x, y });
                    }
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        self.mouse_down = false;
                        self.pending_events.push(InputEvent::MouseUp {
                            x, y, button: MouseButton::Left,
                        });
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                self.pending_events.push(InputEvent::Scroll {
                    x: self.cursor_x,
                    y: self.cursor_y,
                    delta_x: dx,
                    delta_y: dy,
                });
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let key = match event.physical_key {
                    PhysicalKey::Code(code) => match code {
                        KeyCode::Enter => Key::Enter,
                        KeyCode::Escape => Key::Escape,
                        KeyCode::Space => Key::Space,
                        KeyCode::Backspace => Key::Backspace,
                        KeyCode::Tab => Key::Tab,
                        KeyCode::ArrowUp => Key::ArrowUp,
                        KeyCode::ArrowDown => Key::ArrowDown,
                        KeyCode::ArrowLeft => Key::ArrowLeft,
                        KeyCode::ArrowRight => Key::ArrowRight,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
                        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
                        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
                        KeyCode::SuperLeft | KeyCode::SuperRight => Key::Meta,
                        _ => {
                            if let Some(text) = &event.text {
                                if let Some(c) = text.chars().next() {
                                    Key::Char(c)
                                } else {
                                    return;
                                }
                            } else {
                                return;
                            }
                        }
                    },
                    _ => return,
                };
                let ev = match event.state {
                    ElementState::Pressed  => InputEvent::KeyDown { key },
                    ElementState::Released => InputEvent::KeyUp   { key },
                };
                self.pending_events.push(ev);

                if let (ElementState::Pressed, Some(text)) = (event.state, event.text) {
                    for c in text.chars() {
                        self.pending_events.push(InputEvent::Text { character: c });
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            _ => {}
        }
    }

    /// Called after all pending events are processed. Only redraws if an atom
    /// change requested a frame (e.g. from a background animation timer).
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if rosace_state::take_frame_requested() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

/// The physical size our canvas/presenter should use. On every platform but
/// iOS this is just `window.inner_size()`. On iOS we need the TRUE full-screen
/// size (the safe area is applied as Scaffold padding, not by shrinking the
/// canvas — see `sync_ios_safe_area`) — but `window.outer_size()` is NOT a
/// reliable source of that on this winit/iOS-simulator combination: verified
/// by cross-checking against `current_monitor().size()` (a separate winit
/// code path) and the actual simulator screenshot resolution. On iPhone 15
/// Pro sim, `outer_size()` reported 1260x2280 physical while the monitor API
/// and real screenshots agree on 1179x2556 — `outer_size()`/`inner_size()` are
/// wrong in ABSOLUTE terms here (a winit bug in `screen_frame()`'s coordinate
/// conversion, not our math), which showed up as a widget on the right edge
/// (18pt inset from a canvas 27pt too wide) getting clipped by the real
/// screen. `current_monitor().size()` is independently correct IN PORTRAIT,
/// so use it as the canvas size; keep `outer_size() - inner_size()` for the
/// safe-area INSET (the systematic error cancels in that subtraction — the
/// result, 59pt top / 34pt bottom, matches Apple's published iPhone 15 Pro
/// status-bar + home-indicator constants exactly).
///
/// KNOWN GAP — landscape/rotation: `current_monitor().size()` reads
/// `UIScreen.nativeBounds`, which per Apple's docs is fixed to the device's
/// NATIVE (portrait) orientation and does NOT rotate with the interface — in
/// landscape this feeds a swapped width/height into the canvas/GPU surface
/// and corrupts the frame (confirmed: rotated/garbled UI on-device). A
/// prior version of this function tried to detect orientation from
/// `outer_size()`'s aspect ratio and swap accordingly — REVERTED: that
/// signal turned out stale/unreliable (it broke the already-verified
/// portrait case, rendering the whole UI sideways, without reliably fixing
/// landscape either). Do not re-attempt an aspect-ratio heuristic without a
/// real orientation source (e.g. a direct `UIDevice.orientation`/
/// `windowScene.interfaceOrientation` query via objc2 FFI) verified on an
/// actual rotated device — portrait is solid; landscape is unsupported.
#[cfg(target_os = "ios")]
fn physical_canvas_size(window: &winit::window::Window) -> winit::dpi::PhysicalSize<u32> {
    window.current_monitor().map(|m| m.size()).unwrap_or_else(|| window.outer_size())
}
#[cfg(not(target_os = "ios"))]
fn physical_canvas_size(window: &winit::window::Window) -> winit::dpi::PhysicalSize<u32> {
    window.inner_size()
}

/// Measure the iOS status-bar / Dynamic Island / home-indicator insets and
/// publish them via [`rosace_core::safe_area`].
///
/// `inner_size()`/`inner_position()` vs `outer_size()`/`outer_position()` is
/// the standard way to derive these insets (the same technique Flutter's
/// `MediaQuery.padding` and SwiftUI's `.safeAreaInset` are built on) — the
/// difference between the full screen rect and the OS-reported safe content
/// rect. Paired with `physical_canvas_size` rendering the FULL screen, this
/// is the only source of the inset: the platform layer measures it, the
/// widget layer (`Scaffold`) applies it as ordinary padding. Verified via
/// on-device instrumentation on iPhone 15 Pro (iOS 18 sim): inner=1260x2001
/// @ (0,177), outer=1260x2280 @ (0,0), scale=3 → top=59, bottom=34 logical px
/// — exactly the status bar + home indicator heights.
#[cfg(target_os = "ios")]
fn sync_ios_safe_area(window: &winit::window::Window) {
    let scale = window.scale_factor();
    let outer_size = window.outer_size();
    let outer_pos = window.outer_position().unwrap_or_default();
    let inner_size = window.inner_size();
    let inner_pos = window.inner_position().unwrap_or(outer_pos);

    let top = (inner_pos.y - outer_pos.y).max(0) as f64;
    let left = (inner_pos.x - outer_pos.x).max(0) as f64;
    let bottom = (outer_size.height as i64 - inner_size.height as i64
        - (inner_pos.y - outer_pos.y) as i64).max(0) as f64;
    let right = (outer_size.width as i64 - inner_size.width as i64
        - (inner_pos.x - outer_pos.x) as i64).max(0) as f64;

    rosace_core::set_safe_area(rosace_core::SafeArea {
        top: (top / scale) as f32,
        right: (right / scale) as f32,
        bottom: (bottom / scale) as f32,
        left: (left / scale) as f32,
    });
}
