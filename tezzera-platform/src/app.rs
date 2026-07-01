use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

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

    pub fn run<F>(self, paint_fn: F)
    where
        F: FnMut(&mut SkiaCanvas, &[InputEvent]),
    {
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
            canvas: SkiaCanvas::new(w, h),
            pending_events: Vec::new(),
            frame_counter: 0,
            cursor_x: 0.0,
            cursor_y: 0.0,
            last_frame_time: None,
        };
        event_loop.run_app(&mut app).unwrap();
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
    canvas: SkiaCanvas,
    pending_events: Vec<InputEvent>,
    frame_counter: u64,
    cursor_x: f32,
    cursor_y: f32,
    last_frame_time: Option<Instant>,
}

impl<F: FnMut(&mut SkiaCanvas, &[InputEvent])> ApplicationHandler<FrameRequest> for AppState<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.context = Some(context);
        self.surface = Some(surface);
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

                let surface = self.surface.as_mut().unwrap();
                surface
                    .resize(
                        NonZeroU32::new(phys_w).unwrap(),
                        NonZeroU32::new(phys_h).unwrap(),
                    )
                    .unwrap();

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

                // Canvas lives at physical size; play_picture scales all logical
                // coordinates by `scale` so glyphs and shapes are sharp at full
                // HiDPI resolution without any upscaling blur.
                if self.canvas.width() != phys_w
                    || self.canvas.height() != phys_h
                    || (self.canvas.scale() - scale).abs() > 0.01
                {
                    self.canvas = SkiaCanvas::new_hidpi(phys_w, phys_h, scale);
                }
                let events = std::mem::take(&mut self.pending_events);
                (self.paint_fn)(&mut self.canvas, &events);

                // 1:1 blit — no upscaling needed; play_picture already wrote
                // every physical pixel at the correct HiDPI resolution.
                let mut buffer = surface.buffer_mut().unwrap();
                let pixels = self.canvas.pixels();
                for (i, pixel) in buffer.iter_mut().enumerate() {
                    let r = pixels[i * 4];
                    let g = pixels[i * 4 + 1];
                    let b = pixels[i * 4 + 2];
                    *pixel = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                }
                buffer.present().unwrap();

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
                // Do NOT request_redraw here: no hover state is implemented yet,
                // so repainting on every mouse move burns CPU for no visual change.
                // Re-enable once Button/widget hover highlighting is wired up.
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let (x, y) = (self.cursor_x, self.cursor_y);
                let ev = match state {
                    ElementState::Pressed  => InputEvent::MouseDown { x, y, button: btn },
                    ElementState::Released => InputEvent::MouseUp   { x, y, button: btn },
                };
                self.pending_events.push(ev);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.pending_events.push(InputEvent::Scroll {
                    x: self.cursor_x,
                    y: self.cursor_y,
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
