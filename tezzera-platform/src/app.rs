use std::num::NonZeroU32;
use std::sync::Arc;
use web_time::Instant;

#[cfg(debug_assertions)]
use tezzera_trace::{event::TezzeraTrace, trace};


use tezzera_render::canvas::SkiaCanvas;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window as WinitWindow, WindowAttributes, WindowId};

use crate::event::{InputEvent, Key, MouseButton};

/// Sent to the winit event loop from any thread to wake it from `Wait` sleep.
///
/// `Atom::set()` calls `tezzera_state::request_frame()`, which invokes the
/// registered wakeup closure, which sends this event. The event loop then
/// calls `window.request_redraw()` in the `user_event` handler.
pub struct FrameRequest;

pub struct PlatformWindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// Low-level windowed event loop. Accepts a raw canvas-paint closure.
/// For widget-based apps, use `tezzera::App` from the umbrella crate instead.
pub struct PlatformWindow {
    config: PlatformWindowConfig,
}

impl PlatformWindow {
    pub fn new() -> Self {
        Self {
            config: PlatformWindowConfig {
                title: "Tezzera".to_string(),
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
        tezzera_state::register_wakeup(move || {
            let _ = proxy.send_event(FrameRequest);
        });

        // Request the first frame immediately so the window paints on open.
        tezzera_state::request_frame();

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
    presenter: Option<tezzera_compositor::GpuPresenter>,
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
        let presenter: Option<tezzera_compositor::GpuPresenter> = None;
        #[cfg(not(target_arch = "wasm32"))]
        let presenter = tezzera_compositor::GpuPresenter::new(
            window.clone(),
            self.config.width,
            self.config.height,
        );
        if presenter.is_some() {
            log::info!("tezzera-platform: using GPU compositor (wgpu)");
        } else {
            log::info!("tezzera-platform: GPU compositor unavailable, using softbuffer");
            let context = softbuffer::Context::new(window.clone()).unwrap();
            let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
            self.context = Some(context);
            self.surface = Some(surface);
        }
        self.presenter = presenter;
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
                let phys = window.inner_size();
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
                tezzera_animate::set_frame_dt(dt);
                self.last_frame_time = Some(now);

                #[cfg(debug_assertions)]
                let frame = self.frame_counter;
                self.frame_counter += 1;

                #[cfg(debug_assertions)]
                trace!(TezzeraTrace::FrameStart {
                    frame,
                    timestamp: now,
                });

                // Resize base + overlay canvases to match physical window size.
                if self.canvas.width() != phys_w
                    || self.canvas.height() != phys_h
                    || (self.canvas.scale() - scale).abs() > 0.01
                {
                    self.canvas         = SkiaCanvas::new_hidpi(phys_w, phys_h, scale);
                    self.overlay_canvas = SkiaCanvas::new_hidpi(phys_w, phys_h, scale);
                }

                // Clear overlay to transparent before each frame (D078).
                self.overlay_canvas.clear_transparent();

                let events = std::mem::take(&mut self.pending_events);
                (self.paint_fn)(&mut self.canvas, &mut self.overlay_canvas, &events);

                // Present the frame — GPU multi-layer compositor (D076, D079),
                // with softbuffer fallback that CPU-composites overlay on top.
                if let Some(presenter) = &mut self.presenter {
                    // Per-frame dirtiness drives the compositor's texture cache
                    // (D089): a clean base layer reuses its persistent GPU
                    // texture, and a frame where nothing changed skips the
                    // present entirely. `take_frame_dirty` must run every frame
                    // so the flag resets; the base only repaints (and re-marks)
                    // when the frame loop actually redraws it.
                    let base_dirty = self.canvas.take_frame_dirty();

                    // Refresh the retained scroll layers only when the frame
                    // loop published (it repainted). `None` = clean frame →
                    // keep the retained set so the layers persist unchanged.
                    let refreshed = crate::scroll_layer::take_scroll_layers();
                    let scroll_dirty = refreshed.is_some();
                    if let Some(layers) = refreshed {
                        self.scroll_layers = layers;
                    }

                    // Composite bottom-to-top: base, scroll layers (each placed
                    // at its viewport), then the overlay on top (D090). Scroll
                    // layers re-upload only on a publish frame (scroll_dirty);
                    // otherwise D089 reuses their persistent textures.
                    let mut layers = vec![
                        tezzera_compositor::CompositorLayer::tracked(
                            self.canvas.pixels(), phys_w, phys_h, base_dirty,
                        ),
                    ];
                    for sl in &self.scroll_layers {
                        // Live scroll offset from the non-reactive channel
                        // (physical px). A wheel tick updates this without a
                        // repaint, so a scroll-only frame is a uniform write
                        // over the reused content texture (D090).
                        let off = tezzera_state::scroll_offset(sl.id);
                        layers.push(tezzera_compositor::CompositorLayer::placed(
                            &sl.pixels, sl.width, sl.height,
                            tezzera_compositor::LayerRect {
                                x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3,
                            },
                            (off[0] * scale, off[1] * scale),
                            scroll_dirty,
                        ));
                    }
                    // Skip the overlay layer entirely when nothing drew into it
                    // this frame. When it did draw, treat it as dirty — the
                    // overlay is cleared and replayed every frame, so its pixels
                    // may differ even when the base is clean.
                    if self.overlay_canvas.has_drawn() {
                        layers.push(tezzera_compositor::CompositorLayer::tracked(
                            self.overlay_canvas.pixels(), phys_w, phys_h, true,
                        ));
                    }
                    presenter.present_layers(&layers);
                } else if let Some(surface) = &mut self.surface {
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
                    trace!(TezzeraTrace::FrameEnd {
                        frame,
                        duration,
                        dropped,
                    });
                }
            }

            WindowEvent::Resized(size) => {
                if let Some(presenter) = &mut self.presenter {
                    presenter.resize(size.width, size.height);
                }
                self.pending_events.push(InputEvent::WindowResized {
                    width: size.width,
                    height: size.height,
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
        if tezzera_state::take_frame_requested() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}
