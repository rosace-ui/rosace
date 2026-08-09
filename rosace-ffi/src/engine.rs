//! `Engine` — the safe Rust API a native host drives (D106 Phase 24 Step 1).
//!
//! Wraps `rosace::FrameEngine` (build/paint/input) + `GpuPresenter`
//! (`rosace-compositor`) + the base/overlay `SkiaCanvas`es, replaying the
//! same per-frame sequence `rosace-platform`'s winit loop runs (see
//! `rosace-platform/src/app.rs`'s `RedrawRequested` handler) but driven by
//! explicit `resize`/`input`/`frame` calls instead of a winit event loop.
//!
//! This struct is intentionally NOT exposed as `#[no_mangle] extern "C"`
//! here — only a concrete app crate knows its root `Component`, so the
//! actual `rsc_engine_*` C functions are generated per-app (see
//! `examples/ios_stub.rs` for the pattern). `Engine` is what that ~15 lines
//! of per-app glue calls into.

use rosace_compositor::{CompositorLayer, GpuPresenter, LayerRect};
use rosace_core::Component;
use rosace_render::SkiaCanvas;
use rosace_theme::ThemeData;

use crate::event::RscInputEventFfi;
use crate::surface::RawSurface;

pub struct Engine {
    frame_engine: rosace::FrameEngine,
    presenter: GpuPresenter,
    canvas: SkiaCanvas,
    overlay_canvas: SkiaCanvas,
    scroll_layers: Vec<rosace_platform::ScrollLayer>,
    pending_events: Vec<rosace_platform::InputEvent>,
    width: u32,
    height: u32,
    scale: f32,
    /// Wall-clock of the previous `frame()`, for the real animation `dt`.
    last_frame: Option<std::time::Instant>,
    /// Whether at least one frame has been presented (the first must always go).
    presented_once: bool,
    /// Retained GPU-shapes base frame items (D109 C1): rebuilt only on
    /// painted frames, reused on clean ones — same policy as desktop.
    /// Empty when GPU-shapes is off (CPU pixel path).
    frame_items: Vec<rosace_render::canvas::CanvasFrameItem>,
    /// Monotonic epoch for animated shader `time` uniforms.
    anim_epoch: std::time::Instant,
}

impl Engine {
    /// Builds an engine for `root`, targeting `surface`. Returns `None` if
    /// the host GPU/surface setup fails (mirrors `GpuPresenter::new`).
    /// Sets `theme` as the active theme once, like `App::launch` does.
    pub fn init(root: Box<dyn Component>, theme: ThemeData, surface: RawSurface) -> Option<Box<Engine>> {
        let width = surface.width;
        let height = surface.height;
        let scale = surface.scale;

        rosace_theme::set_theme(theme);

        // Persistence backend (D114/D121): mobile apps enter HERE, not
        // `App::launch`, so the store installs here too. iOS: `$HOME` is
        // the per-app sandbox container, so `Documents/rosace.sqlite`
        // needs no app-name namespacing (and rides the user's device
        // backups). Android: the files dir is only knowable from the JNI
        // host (`context.getFilesDir()`) — plumbing that path through
        // `nativeInit` is deferred alongside Known Issue #16 (Android is
        // parked pre-rendering); until then persistent atoms behave as
        // plain state there, same non-fatal degradation as a failed open.
        #[cfg(target_os = "ios")]
        if let Ok(home) = std::env::var("HOME") {
            let dir = std::path::Path::new(&home).join("Documents");
            let _ = std::fs::create_dir_all(&dir);
            match rosace::storage::Storage::open(dir.join("rosace.sqlite")) {
                Ok(store) => {
                    rosace::core::set_persist_backend(Box::new(store));
                }
                Err(e) => eprintln!("rosace: persistence disabled ({e})"),
            }
        }

        let mut presenter = GpuPresenter::new(surface, width, height)?;
        let mut canvas = SkiaCanvas::new_hidpi(width, height, scale);

        // GPU-shapes mode (D109/Phase 27): built-in shape commands render as
        // SDF pipelines on the GPU base canvas instead of CPU tiny-skia.
        // Desktop enables this in `App::launch`; the mobile host enters here
        // and never did — so on-device every shape re-rasterized on the CPU
        // each frame (measured: ~35ms paint vs ~4ms present at 3x on iOS),
        // pegging the CADisplayLink thread and stalling animations. The
        // overlay/scroll canvases stay tiny-skia (matches desktop until C2).
        // `ROSACE_CPU_SHAPES=1` is the kill switch / A-B lever.
        if std::env::var_os("ROSACE_CPU_SHAPES").is_none() {
            rosace::shader::builtin::register_builtins();
            presenter.set_glyph_gamma(rosace_render::canvas::text_gamma_lut());
            canvas.set_gpu_shapes(true);
        }
        // Eager pipeline compilation (the Impeller lesson): everything queued
        // above compiles now, at startup, never lazily on the first paint.
        rosace_platform::app::drain_shader_registrations(&mut presenter);

        // Dev hot reload (Tier 1): on a mobile dev build, listen for edited
        // source pushed from the dev machine over an adb/devicectl-forwarded
        // socket. Desktop starts this in `App::launch`; mobile enters here.
        #[cfg(all(feature = "rsc-hot", any(target_os = "android", target_os = "ios")))]
        rosace::dev_reload::serve_hot_reload_socket(rosace::dev_reload::DEFAULT_HOT_RELOAD_PORT);

        // Real OS system font — the same default as `App::launch` (D127
        // "environment" track, reversing Phase 32's bundled-Inter default).
        // iOS has no filesystem-readable system font (`system_ui()` only
        // probes desktop/Android paths), so it falls through to bundled
        // Inter there; Android reads the real on-device Roboto.
        let font = rosace_render::FontCache::system_ui()
            .or_else(rosace_render::FontCache::system_mono)
            .unwrap_or_else(rosace_render::FontCache::bundled);

        Some(Box::new(Engine {
            frame_engine: rosace::FrameEngine::new(root, font),
            presenter,
            canvas,
            overlay_canvas: SkiaCanvas::new_hidpi(width, height, scale),
            scroll_layers: Vec::new(),
            pending_events: Vec::new(),
            width,
            height,
            scale,
            last_frame: None,
            presented_once: false,
            frame_items: Vec::new(),
            anim_epoch: std::time::Instant::now(),
        }))
    }

    /// Resizes the surface, presenter, and canvases (e.g. on device rotation
    /// or `viewWillLayoutSubviews`), and updates the safe-area insets (e.g.
    /// from a real `UIView.safeAreaInsets` on iOS — Phase 24 Step 2). The
    /// safe-area update always applies, even when the size/scale portion is
    /// a no-op, since insets can change independently (status bar changes,
    /// keyboard) without a size change.
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        scale: f32,
        safe_area: rosace_core::SafeArea,
    ) {
        rosace_core::set_safe_area(safe_area);

        if width == 0 || height == 0 { return; }
        if self.width == width && self.height == height && (self.scale - scale).abs() < 0.01 {
            return;
        }
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.presenter.resize(width, height);
        // Carry the GPU-shapes flag across recreation — a resized surface
        // silently dropping to CPU shapes would be an invisible mode flip.
        let gpu_shapes = self.canvas.gpu_shapes();
        self.canvas = SkiaCanvas::new_hidpi(width, height, scale);
        self.canvas.set_gpu_shapes(gpu_shapes);
        self.overlay_canvas = SkiaCanvas::new_hidpi(width, height, scale);
    }

    /// Publishes a live OS "environment" push (brightness, accessibility
    /// text scale, bold text, reduce motion, 24-hour format) — called from
    /// the native host whenever the OS reports a change (iOS
    /// `traitCollectionDidChange`, Android `onConfigurationChanged`, desktop
    /// `WindowEvent::ThemeChanged`, web `matchMedia` `"change"`), same shape
    /// as [`Self::resize`]'s safe-area push. Also re-syncs the active theme
    /// (`rosace_theme::sync_system_theme`) so brightness changes take visual
    /// effect immediately, unless the app pinned a `ThemeMode`.
    pub fn set_media_query(&mut self, mq: rosace_core::MediaQuery) {
        rosace_core::set_media_query(mq);
        rosace_theme::sync_system_theme();
    }

    /// Queues input events for the next `frame()` call — mirrors how the
    /// winit path batches `WindowEvent`s between `RedrawRequested`s.
    ///
    /// Lifecycle transitions (D110 Phase 29 Step 1) additionally apply
    /// IMMEDIATELY, not just on the next frame: iOS pauses the display
    /// link once backgrounded (and background Metal work is prohibited),
    /// so a `Background` event only queued for the next `frame()` would
    /// first be seen on RESUME — the exact opposite of "pause work while
    /// backgrounded". The atom write is GPU-free and background-safe; the
    /// event still queues too, so `FrameEngine`'s dispatch sees the same
    /// ordered stream on its next frame (re-writing the same value is a
    /// harmless no-op).
    pub fn input(&mut self, events: &[RscInputEventFfi]) {
        for &e in events {
            let event: rosace_platform::InputEvent = e.into();
            if let rosace_platform::InputEvent::Lifecycle(state) = event {
                rosace_core::set_app_lifecycle(state);
            }
            self.pending_events.push(event);
        }
    }

    /// This engine's current semantic tree (D132) — the accessibility model
    /// the native host republishes to VoiceOver/TalkBack. Reads the render
    /// tree, so it is only meaningful after at least one `frame()`.
    pub fn semantics(&self) -> rosace_core::SemanticNode {
        self.frame_engine.semantics()
    }

    /// Runs one frame: build/paint/dispatch (via `FrameEngine`), then
    /// composite + present (via `GpuPresenter`) — the same two-step sequence
    /// `rosace-platform/src/app.rs`'s `RedrawRequested` handler runs.
    pub fn frame(&mut self) {
        // Real wall-clock dt EVERY tick (cheap) so a resumed animation eases at
        // true speed — without this, `frame_dt` stays at its 1/60 default and
        // everything advances per-frame (slow motion on a slower device).
        let now = std::time::Instant::now();
        let dt = self.last_frame
            .map(|t| now.duration_since(t).as_secs_f32())
            // 250ms cap (not desktop's 100ms): a slow device can legitimately
            // take >100ms per frame, and a dt clamped below real elapsed makes
            // animations ease in slow motion; still absorbs a resume spike.
            .unwrap_or(1.0 / 60.0)
            .clamp(0.001, 0.25);
        rosace::animate::set_frame_dt(dt);
        self.last_frame = Some(now);

        // Frame-request-driven, exactly like desktop's winit loop (which only
        // redraws on RedrawRequested). The CADisplayLink polls at 60Hz, but a
        // full paint + overlay redraw + GPU present every tick when NOTHING
        // changed pegs the UI thread (brutal on a slow simulator → the "hang":
        // the DevTools FAB overlay alone re-draws + presents every tick). So do
        // nothing unless the engine actually asked for a frame — an atom change,
        // a running animation, a scroll shift/momentum — or there's pending
        // input. This keeps the render policy platform-agnostic (dirty-driven);
        // the host just adapts that signal to its polling display link.
        let has_input = !self.pending_events.is_empty();
        if self.presented_once && !has_input && !rosace_state::take_frame_requested() {
            return;
        }
        self.presented_once = true;

        // The engine owns the overlay clear (it clears exactly when overlay
        // entries repaint), so no unconditional full-window wipe here — that
        // matches desktop and lets a paint-time overlay persist across frames.
        let events = std::mem::take(&mut self.pending_events);
        self.frame_engine.paint(&mut self.canvas, &mut self.overlay_canvas, &events);

        let base_dirty = self.canvas.take_frame_dirty();
        let refreshed = rosace_platform::take_scroll_layers();
        let scroll_dirty = refreshed.is_some();
        if let Some(layers) = refreshed {
            self.scroll_layers = layers;
        }

        if self.canvas.gpu_shapes() {
            // GPU-shapes present path (D109 C1): the base is an ordered item
            // list — shape SDF quads + CPU text/blit segments in command
            // order — not one pixel buffer. Rebuild only on painted frames;
            // reuse the retained set on clean ones. Overlay stays CPU pixels
            // (tiny-skia until C2); scroll content is GPU offscreens (C2).
            if base_dirty {
                self.frame_items = self.canvas.take_frame_items();
            }
            // Live clock into animated shader `time` uniforms; a running one
            // asks for the next frame (the display link reads this flag).
            let now = self.anim_epoch.elapsed().as_secs_f32();
            let mut has_animated = false;
            for it in &mut self.frame_items {
                if let rosace_render::canvas::CanvasFrameItem::Shader(q) = it {
                    if q.animate_time {
                        rosace::shader::materials::patch_time(&mut q.uniforms, now);
                        has_animated = true;
                    }
                }
            }

            // Reborrow the presenter as a local so the scroll loop can render
            // offscreens while `items` holds immutable borrows of the other
            // (disjoint) fields — same shape as the desktop redraw path.
            let presenter = &mut self.presenter;
            let mut items: Vec<rosace_compositor::FrameItem<'_>> = self
                .frame_items
                .iter()
                .map(|it| rosace_platform::app::canvas_item_to_frame(it, base_dirty))
                .collect();
            for sl in &self.scroll_layers {
                let off = rosace_state::scroll_offset(sl.id);
                let dest = LayerRect { x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3 };
                // Content was rasterized at `scale * zoom`, so the live
                // offset scales up by the same factor into texture space.
                let src_offset = (off[0] * self.scale * sl.zoom, off[1] * self.scale * sl.zoom);
                if !sl.items.is_empty() {
                    // GPU-shapes scroll content (D109 C2): render the items
                    // into the offscreen target on publish frames, then
                    // sample it at the live scroll offset. `pixels` is empty
                    // in this mode — reading it would over-run a 0-len slice.
                    if scroll_dirty {
                        let sub: Vec<rosace_compositor::FrameItem<'_>> = sl
                            .items
                            .iter()
                            .map(|it| rosace_platform::app::canvas_item_to_frame(it, true))
                            .collect();
                        presenter.render_offscreen(sl.id, sl.width, sl.height, &sub);
                    }
                    items.push(rosace_compositor::FrameItem::Offscreen(
                        rosace_compositor::OffscreenRef { key: sl.id, dest, src_offset, dirty: scroll_dirty },
                    ));
                } else {
                    items.push(rosace_compositor::FrameItem::Pixels(CompositorLayer::placed(
                        &sl.pixels, sl.width, sl.height, dest, src_offset, scroll_dirty,
                    )));
                }
            }
            if self.overlay_canvas.has_drawn() {
                items.push(rosace_compositor::FrameItem::Pixels(CompositorLayer::tracked(
                    self.overlay_canvas.pixels(), self.width, self.height, true,
                )));
            }
            presenter.present_frame(&items);

            if has_animated {
                rosace_state::request_frame();
            }
        } else {
            // CPU pixel path (ROSACE_CPU_SHAPES=1 kill switch): single
            // full-window base buffer, scroll + overlay placed on top.
            let mut layers = vec![
                CompositorLayer::tracked(self.canvas.pixels(), self.width, self.height, base_dirty),
            ];
            for sl in &self.scroll_layers {
                let off = rosace_state::scroll_offset(sl.id);
                layers.push(CompositorLayer::placed(
                    &sl.pixels, sl.width, sl.height,
                    LayerRect { x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3 },
                    (off[0] * self.scale, off[1] * self.scale),
                    scroll_dirty,
                ));
            }
            if self.overlay_canvas.has_drawn() {
                layers.push(CompositorLayer::tracked(self.overlay_canvas.pixels(), self.width, self.height, true));
            }
            self.presenter.present_layers(&layers);
        }
    }
}
