use web_time::Instant;

use rosace_render::canvas::SkiaCanvas;

use crate::event::{InputEvent, Key, MouseButton};

// Everything below is desktop-only: `AppState`/`ApplicationHandler` is winit's
// OS-event-loop-driven frame path, never constructed on web (`run_web_native`
// owns the whole web frame loop instead — see below).
#[cfg(not(target_arch = "wasm32"))]
use std::num::NonZeroU32;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use rosace_trace::{event::RosaceTrace, trace};
#[cfg(not(target_arch = "wasm32"))]
use winit::application::ApplicationHandler;
#[cfg(not(target_arch = "wasm32"))]
use winit::event::{ElementState, Ime, WindowEvent};
#[cfg(not(target_arch = "wasm32"))]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(not(target_arch = "wasm32"))]
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(not(target_arch = "wasm32"))]
use winit::window::{Theme, Window as WinitWindow, WindowAttributes, WindowId};

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
        // `'static` so the closure can live inside the web `WebState` across
        // rAF callbacks; native `move` closures already satisfy it.
        F: FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent]) + 'static,
    {
        // Web owns its OWN frame loop, no winit: winit's web event loop won't
        // reliably run frames (`request_redraw` is coalesced however it's
        // triggered), so we create the canvas, build the GPU surface straight
        // from it, and drive `FrameEngine::paint` + present from our own
        // `requestAnimationFrame` (see `run_web_native`). Desktop keeps winit.
        #[cfg(target_arch = "wasm32")]
        {
            run_web_native(self.config, paint_fn);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
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
                overlay_frame_items: Vec::new(),
                shader_fallback_warned: false,
                ime_composing: false,
                anim_epoch: Instant::now(),
            };
            event_loop.run_app(&mut app).unwrap();
        }
    }
}

impl Default for PlatformWindow {
    fn default() -> Self {
        Self::new()
    }
}

// Desktop-only: this is winit's `ApplicationHandler`, the OS-event-loop-driven
// frame path. Web never constructs this — `run_web_native` above owns the
// whole web frame loop instead (see the module doc for why winit-web's event
// loop was dropped entirely).
#[cfg(not(target_arch = "wasm32"))]
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
    // Same retention contract as `frame_items`, for the overlay canvas
    // (D109 overlay-GPU support) — appended to the SAME ordered `items`
    // list as the base/scroll content at present time, positioned last
    // (top-most), so `Backdrop` items there still correctly sample
    // "everything drawn before them" in one render pass.
    overlay_frame_items: Vec<rosace_render::canvas::CanvasFrameItem>,
    // Warn-once flag: shader registrations/quads on the softbuffer fallback
    // path (no GPU) are dropped — loud the first time, silent after.
    shader_fallback_warned: bool,
    // True while a real OS IME session (D116 Step 6) is actively
    // composing — suppresses winit's own key->text resolution so a
    // composed CJK sequence doesn't also insert its raw keystrokes.
    ime_composing: bool,
    // Clock origin for `animate_time` shader quads (D109 maturity): the
    // platform patches `elapsed` into the first 4 uniform bytes of every
    // retained animated quad at each present — GPU-resident animation, no
    // CPU repaint. See `DrawCommand::ShaderFill`.
    anim_epoch: Instant,
}

/// One canvas frame item (D109 C1) as a compositor item: quads pass
/// through; CPU segments become placed pixel layers at their bbox.
///
/// `pub` so custom host drivers that replay this per-frame sequence
/// outside the winit loop (e.g. `rosace-ffi`'s mobile `Engine`) share the
/// exact GPU-shapes translation rather than duplicating it.
pub fn canvas_item_to_frame<'a>(
    it: &'a rosace_render::canvas::CanvasFrameItem,
    dirty: bool,
) -> rosace_compositor::FrameItem<'a> {
    match it {
        rosace_render::canvas::CanvasFrameItem::Shader(q) => {
            rosace_compositor::FrameItem::Shader(rosace_compositor::ShaderQuad {
                pipeline: q.pipeline_id,
                rect:     q.rect,
                uniforms: &q.uniforms,
                clip:     q.clip,
            })
        }
        rosace_render::canvas::CanvasFrameItem::Segment { x, y, w, h, pixels } => {
            rosace_compositor::FrameItem::Pixels(
                rosace_compositor::CompositorLayer::placed(
                    pixels, *w, *h,
                    rosace_compositor::LayerRect {
                        x: *x as f32, y: *y as f32, w: *w as f32, h: *h as f32,
                    },
                    (0.0, 0.0),
                    dirty,
                ),
            )
        }
        rosace_render::canvas::CanvasFrameItem::Image { key, pixels, src_w, src_h, dest, opacity, clip } => {
            rosace_compositor::FrameItem::Image(rosace_compositor::ImageQuad {
                key:    *key,
                pixels: &pixels.0,
                src_w:  *src_w,
                src_h:  *src_h,
                dest:   *dest,
                opacity: *opacity,
                clip:   *clip,
            })
        }
        rosace_render::canvas::CanvasFrameItem::Backdrop { rect, radius, blur, tint } => {
            rosace_compositor::FrameItem::Backdrop(rosace_compositor::BackdropQuad {
                rect:   *rect,
                radius: *radius,
                blur:   *blur,
                tint:   rosace_render::gpu_shapes::linear_rgba(*tint),
            })
        }
        rosace_render::canvas::CanvasFrameItem::Glyphs { glyphs, clip } => {
            // Text color converts sRGB->linear here (same convention as
            // shape quads — the shader outputs linear, the surface
            // re-encodes). Bitmaps are read by the compositor only on the
            // atlas's first sight of each key.
            rosace_compositor::FrameItem::Glyphs {
                glyphs: glyphs.iter().map(|g| rosace_compositor::AtlasGlyph {
                    key:    g.key,
                    bitmap: &g.bitmap.1,
                    x: g.x, y: g.y, w: g.w, h: g.h,
                    color: rosace_render::gpu_shapes::linear_rgba(g.color),
                }).collect(),
                clip: *clip,
            }
        }
    }
}

/// Drain queued `rosace-shader` registrations into the presenter's registry
/// (D109) — eager compilation at the frame boundary, converting the typed
/// `ShaderSpec` to the compositor's primitives-only API (its Layer-0
/// zero-rosace-deps contract means it cannot see `rosace-shader` types).
/// Compile+register any shader pipelines queued since the last drain into
/// the presenter. `pub` for the same reason as [`canvas_item_to_frame`]:
/// custom host drivers (mobile `rosace-ffi`) must run it at startup so the
/// built-in GPU-shape pipelines exist before the first present.
pub fn drain_shader_registrations(presenter: &mut rosace_compositor::GpuPresenter) {
    for (id, spec) in rosace_shader::take_pending_shaders() {
        let blend = match spec.blend {
            rosace_shader::BlendMode::Alpha    => rosace_compositor::ShaderBlend::Alpha,
            rosace_shader::BlendMode::Opaque   => rosace_compositor::ShaderBlend::Opaque,
            rosace_shader::BlendMode::Additive => rosace_compositor::ShaderBlend::Additive,
        };
        // Failure is already logged loudly by register_shader; nothing to
        // add here — the pipeline simply isn't registered and any quad
        // referencing it warns once at present time.
        let _ = presenter.register_shader(id.raw(), &spec.wgsl_source, blend, spec.wants_backdrop);
    }
}

// Keyboard/pointer/wheel events captured directly from the canvas (see
// `wire_web_input`) — winit is not involved on web at all; `run_web_native`
// drains this into each frame's input batch.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_INPUT_QUEUE: std::cell::RefCell<Vec<InputEvent>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Map a browser `KeyboardEvent` to our `InputEvent`s: named keys become
/// `KeyDown`/`KeyUp`; a single printable `key` becomes `Text` (skipped under
/// Ctrl/Meta so shortcuts don't type their letter). Mirrors the winit path in
/// `WindowEvent::KeyboardInput`, which never fires on web.
#[cfg(target_arch = "wasm32")]
fn web_key_events(ev: &web_sys::KeyboardEvent, pressed: bool) -> Vec<InputEvent> {
    let k = ev.key();
    let special = match k.as_str() {
        "Enter" => Some(Key::Enter),
        "Backspace" => Some(Key::Backspace),
        "Tab" => Some(Key::Tab),
        "Escape" => Some(Key::Escape),
        "ArrowLeft" => Some(Key::ArrowLeft),
        "ArrowRight" => Some(Key::ArrowRight),
        "ArrowUp" => Some(Key::ArrowUp),
        "ArrowDown" => Some(Key::ArrowDown),
        "Delete" => Some(Key::Delete),
        "Home" => Some(Key::Home),
        "End" => Some(Key::End),
        "Shift" => Some(Key::Shift),
        "Control" => Some(Key::Control),
        "Alt" => Some(Key::Alt),
        "Meta" => Some(Key::Meta),
        _ => None,
    };
    if let Some(key) = special {
        vec![if pressed { InputEvent::KeyDown { key } } else { InputEvent::KeyUp { key } }]
    } else if pressed && k.chars().count() == 1 && !ev.ctrl_key() && !ev.meta_key() {
        vec![InputEvent::Text { character: k.chars().next().unwrap() }]
    } else {
        Vec::new()
    }
}

/// Attach `keydown`/`keyup` listeners to the canvas that push into
/// `WEB_INPUT_QUEUE` and wake the loop — the web keyboard bridge, since winit
/// never surfaces key events on web.
#[cfg(target_arch = "wasm32")]
fn queue_web_input(events: Vec<InputEvent>) {
    if events.is_empty() { return; }
    WEB_INPUT_QUEUE.with(|q| q.borrow_mut().extend(events));
    rosace_state::request_frame();
}

/// Attach ALL input listeners to the canvas — key, pointer, and wheel — since
/// winit's web backend delivers only `CursorMoved` (no clicks, touch, wheel, or
/// keys). Each browser event is translated to an `InputEvent` and queued for
/// the next `redraw` (see `WEB_INPUT_QUEUE`). Pointer events unify mouse and
/// touch; `offsetX/offsetY` are CSS px, which equal the engine's logical
/// coordinate space, so no scaling is needed.
#[cfg(target_arch = "wasm32")]
fn wire_web_input(canvas: &web_sys::HtmlCanvasElement) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let keydown = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        let evs = web_key_events(&ev, true);
        if !evs.is_empty() {
            ev.prevent_default(); // keep Tab/Space/arrows/Backspace from scrolling or navigating away
            queue_web_input(evs);
        }
    });
    let keyup = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
        queue_web_input(web_key_events(&ev, false));
    });

    // Pointer down/move/up (mouse AND touch). No `prevent_default` on down, so
    // the canvas still takes focus (our keydown listener needs it).
    let pdown = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |ev: web_sys::PointerEvent| {
        let (x, y) = (ev.offset_x() as f32, ev.offset_y() as f32);
        queue_web_input(vec![
            InputEvent::MouseMove { x, y }, // seed the hit position, like the touch path does
            InputEvent::MouseDown { x, y, button: MouseButton::Left },
        ]);
    });
    let pmove = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |ev: web_sys::PointerEvent| {
        queue_web_input(vec![InputEvent::MouseMove { x: ev.offset_x() as f32, y: ev.offset_y() as f32 }]);
    });
    let pup = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |ev: web_sys::PointerEvent| {
        queue_web_input(vec![InputEvent::MouseUp {
            x: ev.offset_x() as f32, y: ev.offset_y() as f32, button: MouseButton::Left,
        }]);
    });

    // Wheel -> Scroll. Negate so the scroll handler's own negation nets to
    // "content follows the wheel". (deltaMode 0 = pixels, the common case.)
    let wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |ev: web_sys::WheelEvent| {
        ev.prevent_default(); // stop the page from scrolling under the canvas
        queue_web_input(vec![InputEvent::Scroll {
            x: ev.offset_x() as f32, y: ev.offset_y() as f32,
            delta_x: -ev.delta_x() as f32, delta_y: -ev.delta_y() as f32,
        }]);
    });

    let _ = canvas.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("pointerdown", pdown.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("pointermove", pmove.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("pointerup", pup.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref());
    // Leak: they live for the page's lifetime (one window).
    keydown.forget(); keyup.forget();
    pdown.forget(); pmove.forget(); pup.forget(); wheel.forget();
}

/// Web-native app state. Unlike desktop (winit `AppState`), this owns NOTHING
/// from winit — just the GPU presenter, the two skia canvases, and the scroll
/// layers. Frames are driven by our own `requestAnimationFrame`.
#[cfg(target_arch = "wasm32")]
struct WebState {
    presenter:     Option<rosace_compositor::GpuPresenter>,
    canvas:        SkiaCanvas,
    overlay:       SkiaCanvas,
    scroll_layers: Vec<crate::scroll_layer::ScrollLayer>,
    frame_items:   Vec<rosace_render::canvas::CanvasFrameItem>,
    width:         u32,
    height:        u32,
    scale:         f32,
    anim_epoch:    Instant,
    last_frame:    Option<Instant>,
    paint_fn:      Box<dyn FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent])>,
}

/// Drive ONE web frame: drain our canvas-listener input, paint, present. This
/// is what winit-web couldn't give us reliably — it's called straight from our
/// own rAF, so it runs every animation tick no matter what winit thinks.
#[cfg(target_arch = "wasm32")]
fn web_native_frame(s: &mut WebState) {
    // Real wall-clock dt (web_time::Instant → performance.now()).
    let now = Instant::now();
    let dt = s
        .last_frame
        .map(|t| now.duration_since(t).as_secs_f32())
        .unwrap_or(1.0 / 60.0)
        .clamp(0.001, 0.1);
    rosace_animate::set_frame_dt(dt);
    s.last_frame = Some(now);

    // ALWAYS drain input so keystrokes/clicks are never lost — even on the
    // early frames before the async GPU build has resolved.
    let mut events: Vec<InputEvent> = Vec::new();
    WEB_INPUT_QUEUE.with(|q| events.append(&mut q.borrow_mut()));

    // GPU not installed yet: still run paint so focus/scroll/hover state
    // advances with the input, but there's nothing to present.
    if s.presenter.is_none() {
        (s.paint_fn)(&mut s.canvas, &mut s.overlay, &events);
        let _ = s.canvas.take_frame_dirty();
        let _ = crate::scroll_layer::take_scroll_layers();
        return;
    }

    (s.paint_fn)(&mut s.canvas, &mut s.overlay, &events);

    let base_dirty = s.canvas.take_frame_dirty();
    let refreshed = crate::scroll_layer::take_scroll_layers();
    let scroll_dirty = refreshed.is_some();
    if let Some(l) = refreshed {
        s.scroll_layers = l;
    }

    // GPU-shapes present, mirroring the mobile FFI engine's frame path.
    if base_dirty {
        s.frame_items = s.canvas.take_frame_items();
    }
    let t = s.anim_epoch.elapsed().as_secs_f32();
    for it in &mut s.frame_items {
        if let rosace_render::canvas::CanvasFrameItem::Shader(q) = it {
            if q.animate_time {
                rosace_shader::materials::patch_time(&mut q.uniforms, t);
            }
        }
    }
    let presenter = s.presenter.as_mut().unwrap();
    let mut items: Vec<rosace_compositor::FrameItem<'_>> = s
        .frame_items
        .iter()
        .map(|it| canvas_item_to_frame(it, base_dirty))
        .collect();
    for sl in &s.scroll_layers {
        let off = rosace_state::scroll_offset(sl.id);
        let dest = rosace_compositor::LayerRect { x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3 };
        let src = (off[0] * s.scale * sl.zoom, off[1] * s.scale * sl.zoom);
        if !sl.items.is_empty() {
            if scroll_dirty {
                let sub: Vec<rosace_compositor::FrameItem<'_>> =
                    sl.items.iter().map(|it| canvas_item_to_frame(it, true)).collect();
                presenter.render_offscreen(sl.id, sl.width, sl.height, &sub);
            }
            items.push(rosace_compositor::FrameItem::Offscreen(rosace_compositor::OffscreenRef {
                key: sl.id,
                dest,
                src_offset: src,
                dirty: scroll_dirty,
            }));
        } else {
            items.push(rosace_compositor::FrameItem::Pixels(rosace_compositor::CompositorLayer::placed(
                &sl.pixels, sl.width, sl.height, dest, src, scroll_dirty,
            )));
        }
    }
    if s.overlay.has_drawn() {
        items.push(rosace_compositor::FrameItem::Pixels(rosace_compositor::CompositorLayer::tracked(
            s.overlay.pixels(),
            s.width,
            s.height,
            true,
        )));
    }
    presenter.present_frame(&items);
}

/// Web entry point: create the canvas, build the GPU surface straight from it,
/// wire input, and drive frames from our OWN `requestAnimationFrame` — winit is
/// never involved on web.
#[cfg(target_arch = "wasm32")]
fn run_web_native(
    _config: PlatformWindowConfig,
    paint_fn: impl FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent]) + 'static,
) {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let web_win = web_sys::window().expect("no window");
    let document = web_win.document().expect("no document");
    let dpr = web_win.device_pixel_ratio() as f32;
    let vw = web_win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
    let vh = web_win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(600.0);
    let phys_w = (vw as f32 * dpr).max(1.0) as u32;
    let phys_h = (vh as f32 * dpr).max(1.0) as u32;

    let canvas: web_sys::HtmlCanvasElement =
        document.create_element("canvas").unwrap().dyn_into().unwrap();
    canvas.set_width(phys_w);
    canvas.set_height(phys_h);
    let style = canvas.style();
    let _ = style.set_property("width", &format!("{vw}px"));
    let _ = style.set_property("height", &format!("{vh}px"));
    let _ = style.set_property("display", "block");
    let _ = style.set_property("outline", "none");
    let _ = canvas.set_attribute("tabindex", "0");
    if let Some(body) = document.body() {
        let _ = body.append_child(&canvas);
    }
    let _ = canvas.focus();
    wire_web_input(&canvas);

    let state = Rc::new(RefCell::new(WebState {
        presenter:     None,
        canvas:        SkiaCanvas::new_hidpi(phys_w, phys_h, dpr),
        overlay:       SkiaCanvas::new_hidpi(phys_w, phys_h, dpr),
        scroll_layers: Vec::new(),
        frame_items:   Vec::new(),
        width:         phys_w,
        height:        phys_h,
        scale:         dpr,
        anim_epoch:    Instant::now(),
        last_frame:    None,
        paint_fn:      Box::new(paint_fn),
    }));

    // Async GPU build straight from the canvas, then enable GPU-shapes.
    {
        let state = state.clone();
        let canvas = canvas.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match rosace_compositor::GpuPresenter::new_async_canvas(canvas, phys_w, phys_h).await {
                Some(mut p) => {
                    rosace_shader::builtin::register_builtins();
                    p.set_glyph_gamma(rosace_render::canvas::text_gamma_lut());
                    drain_shader_registrations(&mut p);
                    let mut s = state.borrow_mut();
                    s.canvas.set_gpu_shapes(true);
                    s.presenter = Some(p);
                    log::info!("rosace-platform(web): GPU compositor installed");
                }
                None => log::error!("rosace-platform(web): GPU init FAILED — no frames will present"),
            }
        });
    }

    // Keep the canvas + presenter matched to the viewport on resize.
    {
        let state = state.clone();
        let canvas = canvas.clone();
        let onresize = Closure::<dyn FnMut()>::new(move || {
            let Some(w) = web_sys::window() else { return };
            let dpr = w.device_pixel_ratio() as f32;
            let vw = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
            let vh = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(600.0);
            let pw = (vw as f32 * dpr).max(1.0) as u32;
            let ph = (vh as f32 * dpr).max(1.0) as u32;
            let mut s = state.borrow_mut();
            if s.width == pw && s.height == ph && (s.scale - dpr).abs() < 0.01 {
                return;
            }
            canvas.set_width(pw);
            canvas.set_height(ph);
            let st = canvas.style();
            let _ = st.set_property("width", &format!("{vw}px"));
            let _ = st.set_property("height", &format!("{vh}px"));
            s.width = pw;
            s.height = ph;
            s.scale = dpr;
            if let Some(p) = s.presenter.as_mut() {
                p.resize(pw, ph);
            }
            let has_gpu = s.presenter.is_some();
            s.canvas = SkiaCanvas::new_hidpi(pw, ph, dpr);
            s.canvas.set_gpu_shapes(has_gpu);
            s.overlay = SkiaCanvas::new_hidpi(pw, ph, dpr);
        });
        web_win.set_onresize(Some(onresize.as_ref().unchecked_ref()));
        onresize.forget();
    }

    // Live OS light/dark + reduce-motion (D127 "environment" track) via
    // `matchMedia` — same shape as the resize closure above: read once at
    // startup, then a "change" listener keeps it live with no reload. Web
    // has no OS-wide accessibility text-scale/bold-text concept exposed to
    // the page (unlike iOS/Android), so those `MediaQuery` fields stay at
    // their documented default here.
    {
        fn push_web_media_query() {
            let Some(w) = web_sys::window() else { return };
            let is_dark = w.match_media("(prefers-color-scheme: dark)").ok().flatten()
                .map(|m| m.matches()).unwrap_or(false);
            let reduce_motion = w.match_media("(prefers-reduced-motion: reduce)").ok().flatten()
                .map(|m| m.matches()).unwrap_or(false);
            let mut mq = rosace_core::media_query::use_media_query();
            mq.is_dark = is_dark;
            mq.reduce_motion = reduce_motion;
            rosace_core::set_media_query(mq);
            rosace_theme::sync_system_theme();
        }
        push_web_media_query();

        let onchange = Closure::<dyn FnMut()>::new(push_web_media_query);
        for query in ["(prefers-color-scheme: dark)", "(prefers-reduced-motion: reduce)"] {
            if let Ok(Some(mql)) = web_win.match_media(query) {
                let _ = mql.add_event_listener_with_callback("change", onchange.as_ref().unchecked_ref());
            }
        }
        onchange.forget();
    }

    // THE loop: our own rAF, self-rescheduling. This is the whole point of
    // dropping winit — the frame runs every tick, so queued input always drains.
    let cb: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let cb2 = cb.clone();
    *cb.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        web_native_frame(&mut state.borrow_mut());
        if let Some(w) = web_sys::window() {
            if let Some(c) = cb2.borrow().as_ref() {
                let _ = w.request_animation_frame(c.as_ref().unchecked_ref());
            }
        }
    }) as Box<dyn FnMut()>));
    {
        let b = cb.borrow();
        if let Some(c) = b.as_ref() {
            let _ = web_win.request_animation_frame(c.as_ref().unchecked_ref());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F: FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent])> AppState<F> {
    /// Resize the GPU surface/canvases to the current physical window size,
    /// run one paint pass, and present. Called from `RedrawRequested` (the
    /// normal per-frame path) AND synchronously from `WindowEvent::Resized`
    /// (see the comment there for why the latter matters on macOS).
    fn redraw(&mut self) {
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
            self.overlay_canvas.set_gpu_shapes(gpu_shapes);
        }

        // Overlay clearing is ENGINE-owned (2026-07-19): the engine
        // clears exactly when it repaints overlay entries (or when a
        // repaint frame declared none), so an open paint-time overlay
        // (Dropdown menu) survives engine-skipped animated presents with
        // its pixels retained — and idle frames never pay the full-window
        // tiny-skia fill that was ~40% of a debug core.

        let events = std::mem::take(&mut self.pending_events);
        (self.paint_fn)(&mut self.canvas, &mut self.overlay_canvas, &events);

        // Position the OS's CJK candidate window near the real
        // caret (D116 Step 6) — `rosace_core::ime_cursor_area()`
        // is a `GlobalAtom` bridge `TextInput`/`TextArea` publish
        // to every paint while focused (see `ime_hint.rs` for why
        // it lives in `rosace-core`, the lowest layer both this
        // crate and `rosace-widgets` share). Logical -> physical
        // via the same `scale` used for the canvas itself.
        if let Some(rect) = rosace_core::ime_cursor_area() {
            window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(
                    (rect.origin.x * scale) as i32,
                    (rect.origin.y * scale) as i32,
                ),
                winit::dpi::PhysicalSize::new(
                    (rect.size.width * scale).max(1.0) as u32,
                    (rect.size.height * scale).max(1.0) as u32,
                ),
            );
        }

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
            // Overlay frame items (D109 overlay-GPU support, 2026-08-04):
            // same retention contract as the base canvas's `frame_items`
            // above — refreshed only on frames the overlay pass actually
            // repainted (`overlay_dirty`), retained across the frames it
            // didn't (the overlay is cleared+replayed only when something
            // opened/closed/changed, not every present). Pushed into the
            // SAME ordered `items` list as base+scroll content, below.
            let overlay_dirty = self.overlay_canvas.take_frame_dirty();
            if self.overlay_canvas.gpu_shapes() {
                if overlay_dirty {
                    self.overlay_frame_items = self.overlay_canvas.take_frame_items();
                }
            } else {
                self.overlay_frame_items.clear();
                // No CPU-fallback rendering path for ShaderFill content
                // recorded while the overlay canvas itself isn't in
                // GPU-shapes mode (e.g. `ROSACE_CPU_SHAPES=1`) — drain so
                // quads can't accumulate, loud once if anything shows up.
                let overlay_quads = self.overlay_canvas.take_shader_quads();
                if !overlay_quads.is_empty() && !self.shader_fallback_warned {
                    self.shader_fallback_warned = true;
                    log::warn!(
                        "rosace-platform: {} ShaderFill command(s) recorded in the \
                         OVERLAY pass were dropped (GPU shapes disabled)",
                        overlay_quads.len(),
                    );
                }
            }

            // GPU-resident animation (D109 maturity): patch the live
            // clock into every retained `animate_time` quad's first 4
            // uniform bytes (the `time`-first convention). This is the
            // whole per-frame cost of continuous shader animation — a
            // 4-byte write here plus the compositor's uniform upload; no
            // widget repaints, no tree walk, no rasterization.
            let now = self.anim_epoch.elapsed().as_secs_f32();
            let mut has_animated_quads = false;
            for it in &mut self.frame_items {
                if let rosace_render::canvas::CanvasFrameItem::Shader(q) = it {
                    if q.animate_time {
                        rosace_shader::materials::patch_time(&mut q.uniforms, now);
                        has_animated_quads = true;
                    }
                }
            }
            for q in &mut self.shader_quads {
                if q.animate_time {
                    rosace_shader::materials::patch_time(&mut q.uniforms, now);
                    has_animated_quads = true;
                }
            }
            for it in &mut self.overlay_frame_items {
                if let rosace_render::canvas::CanvasFrameItem::Shader(q) = it {
                    if q.animate_time {
                        rosace_shader::materials::patch_time(&mut q.uniforms, now);
                        has_animated_quads = true;
                    }
                }
            }
            // Animated quads INSIDE retained scroll layers (D109 C2): patch
            // them too, and remember which layers held one — those need
            // their offscreen content re-rendered this frame (below), since
            // a non-publish frame otherwise reuses the offscreen texture.
            let mut animated_layers: std::collections::HashSet<u64> = std::collections::HashSet::new();
            for sl in &mut self.scroll_layers {
                for it in &mut sl.items {
                    if let rosace_render::canvas::CanvasFrameItem::Shader(q) = it {
                        if q.animate_time {
                            rosace_shader::materials::patch_time(&mut q.uniforms, now);
                            has_animated_quads = true;
                            animated_layers.insert(sl.id);
                        }
                    }
                }
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
                    items.push(canvas_item_to_frame(it, base_dirty));
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
                if !sl.items.is_empty() {
                    // GPU-shapes scroll content (D109 C2): render the
                    // items into the offscreen target on publish frames —
                    // or every frame for a layer holding an animated quad
                    // (its patched time uniform must reach the texture).
                    let layer_dirty = scroll_dirty || animated_layers.contains(&sl.id);
                    if layer_dirty {
                        let sub: Vec<rosace_compositor::FrameItem<'_>> = sl.items
                            .iter()
                            .map(|it| canvas_item_to_frame(it, true))
                            .collect();
                        presenter.render_offscreen(sl.id, sl.width, sl.height, &sub);
                    }
                    items.push(rosace_compositor::FrameItem::Offscreen(
                        rosace_compositor::OffscreenRef {
                            key: sl.id,
                            dest: rosace_compositor::LayerRect {
                                x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3,
                            },
                            src_offset: (off[0] * scale * sl.zoom, off[1] * scale * sl.zoom),
                            dirty: layer_dirty,
                        },
                    ));
                } else {
                    items.push(rosace_compositor::FrameItem::Pixels(
                        rosace_compositor::CompositorLayer::placed(
                            &sl.pixels, sl.width, sl.height,
                            rosace_compositor::LayerRect {
                                x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3,
                            },
                            (off[0] * scale * sl.zoom, off[1] * scale * sl.zoom),
                            scroll_dirty,
                        ),
                    ));
                }
            }
            // Overlay, on top of everything (D090) — pushed into the SAME
            // ordered `items` list (not a separate present call) so a
            // `Backdrop` item recorded in the overlay pass (a glass
            // Drawer/Sheet/Dialog material) still correctly samples
            // "everything drawn before it" from the real composited scene,
            // not an empty overlay-only pass.
            if self.overlay_canvas.gpu_shapes() {
                for it in &self.overlay_frame_items {
                    items.push(canvas_item_to_frame(it, overlay_dirty));
                }
            } else if self.overlay_canvas.has_drawn() {
                // Skip the layer entirely when nothing drew into it this
                // frame. When it did draw, treat it as dirty — the overlay
                // is cleared and replayed every frame it draws at all, so
                // its pixels may differ even when the base is clean.
                items.push(rosace_compositor::FrameItem::Pixels(
                    rosace_compositor::CompositorLayer::tracked(
                        self.overlay_canvas.pixels(), phys_w, phys_h, true,
                    ),
                ));
            }
            presenter.present_frame(&items);

            // An animated quad needs the NEXT frame too. NOT
            // `window.request_redraw()` — called from inside
            // `RedrawRequested`, macOS coalesces it into the frame already
            // in flight and the loop dies after one present (found live:
            // frozen animation at 0% CPU). `request_frame` goes through
            // the event-loop proxy (`FrameRequest` user event), the same
            // path engine-side animation wakeups use, so the redraw lands
            // AFTER this handler returns.
            //
            // Throttled to ~30fps: ambient shader animation doesn't need a
            // 120Hz ProMotion cadence, and each present of a scene with a
            // backdrop material re-renders the whole scene texture — at
            // full refresh that alone was ~70% of a debug-build core
            // (measured live). One timer thread per presented frame, so
            // the steady state is 30 wakeups/second, not a spawn storm.
            if has_animated_quads {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(33));
                    rosace_state::request_frame();
                });
            }
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
}

#[cfg(not(target_arch = "wasm32"))]
impl<F: FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &[InputEvent])> ApplicationHandler<FrameRequest> for AppState<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            // Open focused + in front (matters for `rsc dev`, which launches the
            // app from a terminal — otherwise the window opens behind it).
            .with_active(true)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        window.focus_window();

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

        push_desktop_theme(window.theme());

        // Try GPU compositor (D072). Fall back to softbuffer if unavailable.
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
        // Enable OS IME globally for this window (D116 Step 6) — not
        // scoped to "only while a text field is focused": there is no
        // existing channel for per-field enable/disable (would need the
        // same engine->platform bridge `ime_cursor_area` uses, gated on
        // focus, which is a real follow-up but not required for real
        // CJK composition to work). Composing only actually ENGAGES the
        // OS candidate window when the user types a composing sequence,
        // so leaving it globally allowed is the same posture VS Code and
        // most desktop editors take.
        window.set_ime_allowed(true);

        self.presenter = presenter;
        if let Some(p) = self.presenter.as_mut() {
            // GPU-shapes mode (D109/Phase 27 Step 3): built-in shape
            // commands render as SDF pipelines. Scroll content (C2) and
            // the overlay canvas (2026-08-04, this pass) both propagate
            // the same flag — every canvas stays in sync, no silent
            // per-canvas fallback to CPU. `ROSACE_CPU_SHAPES=1` is the
            // kill switch (and the A/B measurement lever): full tiny-skia
            // path everywhere, as before Step 3.
            if std::env::var_os("ROSACE_CPU_SHAPES").is_none() {
                rosace_shader::builtin::register_builtins();
                // One gamma curve for both text paths (D109 Step 4).
                p.set_glyph_gamma(rosace_render::canvas::text_gamma_lut());
                self.canvas.set_gpu_shapes(true);
                self.overlay_canvas.set_gpu_shapes(true);
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

            WindowEvent::RedrawRequested => self.redraw(),

            WindowEvent::ThemeChanged(theme) => {
                push_desktop_theme(Some(theme));
                if let Some(w) = &self.window { w.request_redraw(); }
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
                // Redraw SYNCHRONOUSLY here, not just via `request_redraw()`.
                // During a native macOS live-resize drag, AppKit runs its own
                // nested tracking runloop and just stretches the last
                // presented GPU frame to fill the growing/shrinking window
                // until the app actually submits a new one — `request_redraw`
                // only *schedules* a `RedrawRequested` for the next winit
                // event-loop turn, which can lag many resize ticks behind
                // during that nested loop. The visible symptom (confirmed
                // live) was exactly this: ghosted/duplicated stale content
                // and uninitialized swapchain pixels bleeding through at the
                // window edges while dragging. Drawing immediately, once per
                // resize tick, keeps the presented frame in lockstep with the
                // window frame the OS is already showing.
                self.redraw();
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
                // `PixelDelta` (trackpad) is reported in PHYSICAL pixels,
                // same as `CursorMoved`/`Touch` — dividing by scale_factor
                // is what those two already do to land in the logical-pixel
                // layout space the scroll controllers work in. Missing here
                // made every trackpad scroll on a Retina/HiDPI display (any
                // scale_factor > 1, i.e. most Macs) cover 2x the intended
                // logical distance — user-reported as "scroll has a
                // momentum issue" (it wasn't momentum, it was distance).
                // `LineDelta` (a physical mouse wheel's "notches") is
                // already device-independent and needs no such conversion.
                let scale = self.window.as_ref()
                    .map(|w| w.scale_factor())
                    .unwrap_or(1.0);
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => ((p.x / scale) as f32, (p.y / scale) as f32),
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

            // Real trackpad pinch-to-zoom (macOS/iOS only — winit's own
            // platform-support note). `phase` (Started/Moved/Ended) isn't
            // distinguished here: Moved carries the real per-tick delta,
            // Started/Ended report ~0, which is a harmless no-op multiply
            // through InteractiveViewer's `zoom *= 1.0 + delta`.
            WindowEvent::PinchGesture { delta, .. } => {
                self.pending_events.push(InputEvent::Pinch {
                    x: self.cursor_x,
                    y: self.cursor_y,
                    delta: delta as f32,
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
                        KeyCode::Delete => Key::Delete,
                        KeyCode::Home => Key::Home,
                        KeyCode::End => Key::End,
                        KeyCode::F12 => Key::F12,
                        KeyCode::F11 => Key::F11,
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

                // While an IME session is actively composing (D116 Step
                // 6), suppress winit's own key->text resolution — the
                // composed result arrives through `WindowEvent::Ime`
                // instead, and pushing BOTH would double-insert (e.g. the
                // raw romaji AND the composed kana).
                if !self.ime_composing {
                    if let (ElementState::Pressed, Some(text)) = (event.state, event.text) {
                        for c in text.chars() {
                            self.pending_events.push(InputEvent::Text { character: c });
                        }
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::Ime(ime) => {
                let ev = match ime {
                    Ime::Enabled => rosace_ime::ImeEvent::Enabled,
                    Ime::Preedit(text, cursor_range) => {
                        self.ime_composing = !text.is_empty();
                        rosace_ime::ImeEvent::Preedit { text, cursor_range }
                    }
                    Ime::Commit(text) => {
                        self.ime_composing = false;
                        rosace_ime::ImeEvent::Commit(text)
                    }
                    Ime::Disabled => {
                        self.ime_composing = false;
                        rosace_ime::ImeEvent::Disabled
                    }
                };
                self.pending_events.push(InputEvent::Ime(ev));
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
        // Web self-sustains its rAF loop from inside `redraw`, so this only
        // matters on desktop (idle until an OS event or `request_frame`).
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
// `AppState` (this fn's only caller) is native-only — web never builds a
// winit window, so wasm32 is excluded here even though it'd already fall
// under "not ios".
#[cfg(all(not(target_os = "ios"), not(target_arch = "wasm32")))]
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

/// Pushes the OS light/dark setting from winit's own `Window::theme()` /
/// `WindowEvent::ThemeChanged` (macOS/Windows/Linux all report this
/// natively through winit — no platform-specific code or new dependency
/// needed) and re-syncs the active theme. `None` (theme undetermined,
/// reported by some window managers) leaves `is_dark` at its current value
/// rather than guessing.
///
/// Desktop has no OS-wide accessibility text-scale/bold-text/reduce-motion
/// concept exposed through winit, so every other `MediaQuery` field stays
/// at its documented default here — not a gap, see `media_query.rs`'s doc.
#[cfg(not(target_arch = "wasm32"))]
fn push_desktop_theme(theme: Option<Theme>) {
    let Some(theme) = theme else { return };
    let mut mq = rosace_core::media_query::use_media_query();
    mq.is_dark = theme == Theme::Dark;
    rosace_core::set_media_query(mq);
    rosace_theme::sync_system_theme();
    rosace_trace::debug!(
        "OS theme -> {theme:?} (mode={:?}, active_theme.is_dark={})",
        rosace_theme::use_theme_mode(), rosace_theme::use_theme().is_dark,
    );
}
