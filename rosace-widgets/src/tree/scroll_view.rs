use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::{Color, DrawCommand};
use rosace_scroll::{ScrollController, ScrollPhysics, ScrollStyle};
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h, intersect_rect};

/// Scroll direction.
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

/// Maximum content extent (logical px) on the scroll axis that the GPU-layer
/// path (D090) can composite as a single placed texture. Content taller than
/// this automatically uses the base (CPU-painted) path instead — correct,
/// just without the zero-repaint scroll optimization — via the
/// automatic-default heuristic (`should_auto_gpu`), so it never silently
/// mis-renders.
///
/// This is intentionally NOT solved with GPU-layer re-render windowing (a
/// moving texture window re-rendered as scroll approaches its edge). For
/// content that's actually large because it's a LONG LIST, [`super::ListView`]
/// already solves this the better way: real virtualization — only the rows
/// intersecting the viewport are ever built, laid out, or painted (O(visible)
/// cost, no texture-size limit to hit at all, since the full content is never
/// materialized). Reach for `ListView::builder` for that case rather than
/// wrapping a huge item list in a `ScrollView`. The base-path fallback here
/// exists for the much narrower remaining case — one large *non-virtualized*
/// widget subtree (e.g. a single big `Image`) — where it's correct but not
/// GPU-accelerated.
pub const MAX_TL_DIM: f32 = 4096.0;

/// A scrollable viewport. The child can exceed the available size; content
/// is painted at the scroll offset and clipped to the viewport bounds.
///
/// Scrolls by default (D101): the position lives on the widget's render-tree
/// node and survives rebuilds — no wiring needed. Pass a
/// [`ScrollController`] (`::controlled` / `.controller()`) only when the app
/// needs programmatic control.
///
/// The GPU-composited layer path (D090) is now the TRANSPARENT DEFAULT for
/// plain [`ScrollView::new`] scroll views: once content is measured, a scroll
/// view whose content actually overflows the viewport on the scroll axis and
/// stays within [`MAX_TL_DIM`] automatically composites as a placed GPU layer
/// (scrolling becomes a compositor UV shift, zero component repaint) — no
/// `.gpu_layer()` call needed. Content that doesn't overflow, or that exceeds
/// `MAX_TL_DIM`, uses the base (CPU-painted) path automatically. `::fixed`
/// and `::controlled` always use the base path — programmatic control and
/// snapshot modes need exact, un-composited semantics.
pub struct ScrollView {
    child: BoxedWidget,
    /// Fixed offset for [`ScrollView::fixed`] snapshot mode.
    fixed_offset: Option<f32>,
    /// Explicit controller override (D101). `None` = implicit node controller.
    controller: Option<ScrollController>,
    pub axis: ScrollAxis,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
    /// Force the GPU-layer path on even when the automatic heuristic
    /// (`should_auto_gpu`) would not have chosen it (e.g. content smaller
    /// than the viewport that the app still wants pre-composited). The
    /// automatic default (see struct docs) already enables it when it helps;
    /// this flag is now an override for the exceptional case, not the only
    /// way to get the GPU path.
    gpu_layer: bool,
    /// Explicit physics override (D108/Phase 26 Step 2). `None` resolves via
    /// [`resolve_physics`] — the app's theme `ext` value, else a per-platform
    /// default. Always the highest-priority source when set.
    physics: Option<ScrollPhysics>,
}

/// Resolves the physics a [`ScrollView`] actually uses: an explicit
/// `.physics(...)` always wins, then the app's own theme override (a
/// `ScrollStyle` stashed via `ThemeData::with_ext`), then a per-platform
/// default — never a hardcoded platform branch in widget code itself (see
/// `.steering/PHASE_26.md` Step 2).
pub fn resolve_physics(theme: &rosace_theme::ThemeData, explicit: Option<ScrollPhysics>) -> ScrollPhysics {
    explicit
        .or_else(|| theme.ext::<ScrollStyle>().map(|s| s.physics))
        .unwrap_or_else(|| ScrollStyle::default_for_platform(rosace_core::use_platform()))
}

impl ScrollView {
    /// A vertical scroll view. Just scrolls — position is implicit per-node
    /// state (D101). Automatically GPU-composited once content overflows the
    /// viewport and fits within [`MAX_TL_DIM`] (see struct docs) — no
    /// `.gpu_layer()` call needed for the common case.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            fixed_offset: None,
            controller: None,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
            gpu_layer: false,
            physics: None,
        }
    }

    /// Override the scroll physics (drag-to-pan momentum + overscroll
    /// behavior) regardless of the platform default — see
    /// [`resolve_physics`]. Base (CPU) path only; no effect in GPU-layer
    /// mode, which doesn't yet have drag-to-pan (D108/Phase 26 Step 2).
    pub fn physics(mut self, p: ScrollPhysics) -> Self {
        self.physics = Some(p);
        self
    }

    /// Force the GPU-layer path on regardless of the automatic size
    /// heuristic (see struct docs — [`ScrollView::new`] already auto-detects
    /// the common case). Content is capped at [`MAX_TL_DIM`]; taller content
    /// silently falls back to the base path (windowing is not yet built).
    pub fn gpu(child: impl Widget + 'static) -> Self {
        Self { gpu_layer: true, ..Self::new(child) }
    }

    /// Force GPU-layer compositing on (see [`ScrollView::gpu`]).
    pub fn gpu_layer(mut self) -> Self { self.gpu_layer = true; self }

    /// A horizontal scroll view — carousels, chip rows, code blocks.
    pub fn horizontal(child: impl Widget + 'static) -> Self {
        Self { axis: ScrollAxis::Horizontal, ..Self::new(child) }
    }

    /// A snapshot viewport — never responds to input. Set the offset with
    /// `.offset(px)`. For golden tests and static mockups.
    pub fn fixed(child: impl Widget + 'static) -> Self {
        Self { fixed_offset: Some(0.0), ..Self::new(child) }
    }

    /// A scroll view driven by an explicit [`ScrollController`] —
    /// programmatic scroll_to / scroll_by / scroll_to_top / scroll_to_bottom.
    /// Create the controller with `ScrollController::for_ctx(ctx)`.
    pub fn controlled(child: impl Widget + 'static, controller: ScrollController) -> Self {
        Self { controller: Some(controller), ..Self::new(child) }
    }

    /// Attach an explicit controller (same as [`ScrollView::controlled`]).
    pub fn controller(mut self, c: ScrollController) -> Self {
        self.controller = Some(c);
        self
    }

    /// Fixed-mode offset in logical pixels (only meaningful with `fixed`).
    pub fn offset(mut self, o: f32) -> Self { self.fixed_offset = Some(o); self }

    pub fn axis(mut self, a: ScrollAxis) -> Self { self.axis = a; self }
    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
    pub fn scrollbar_color(mut self, c: Color) -> Self { self.scrollbar_color = c; self }

    /// Content constraints (unbounded-axis doctrine, API_DESIGN §6): on the
    /// scroll axis min = viewport, max = Unbounded. Shared by both the GPU
    /// and base paint paths so content is measured identically either way.
    fn child_constraints(&self, vp: Rect) -> Constraints {
        use rosace_layout::AxisBound;
        match self.axis {
            ScrollAxis::Vertical => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Bounded(vp.size.width),
                min_height: vp.size.height,
                max_height: AxisBound::Unbounded,
            },
            ScrollAxis::Horizontal => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Unbounded,
                min_height: vp.size.height,
                max_height: AxisBound::Bounded(vp.size.height),
            },
            ScrollAxis::Both => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Unbounded,
                min_height: vp.size.height,
                max_height: AxisBound::Unbounded,
            },
        }
    }

    /// The automatic-default heuristic (D090 transparent default): GPU-layer
    /// compositing helps only when there is actually something to scroll
    /// (content overflows the viewport on the scroll axis) and only when the
    /// content fits in a single placed texture ([`MAX_TL_DIM`] — taller
    /// content needs re-render windowing, not yet built, so it must stay on
    /// the base path rather than silently mis-render).
    fn should_auto_gpu(&self, vp: Size, child_size: Size) -> bool {
        let (overflow, extent) = match self.axis {
            ScrollAxis::Vertical => (child_size.height > vp.height, child_size.height),
            ScrollAxis::Horizontal => (child_size.width > vp.width, child_size.width),
            ScrollAxis::Both => (
                child_size.height > vp.height || child_size.width > vp.width,
                child_size.height.max(child_size.width),
            ),
        };
        overflow && extent <= MAX_TL_DIM
    }

    /// GPU-layer paint path (D090). Records the content once into its own
    /// sub-tree/picture at content-local `(0,0)`, attaches it as a
    /// TransformLayer entry (the platform composites it as a placed layer), and
    /// registers wheel scrolling straight into the non-reactive offset channel
    /// so a scroll tick is a compositor UV shift with no component repaint.
    /// `child_size` is measured once by the caller ([`Widget::paint`]) and
    /// passed in — this never re-measures.
    fn paint_gpu(&self, ctx: &mut PaintCtx, child_size: Size) {
        use super::TransformLayerEntry;
        let vp = ctx.rect;
        let node_id = ctx.node as u64;
        let off = rosace_state::scroll_offset(node_id);

        // Record the content at (0,0) into its own node/picture (D090).
        let sub_node = ctx.tree.borrow_mut().slot(ctx.node, true);
        let mut sub_rec = rosace_render::PictureRecorder::new();
        let child_rect = Rect { origin: Point { x: 0.0, y: 0.0 }, size: child_size };
        let mut sub_ctx = PaintCtx {
            recorder: &mut sub_rec,
            rect: child_rect,
            font: ctx.font,
            theme: ctx.theme.clone(),
            tree: ctx.tree.clone(),
            node: sub_node,
            owner: ctx.owner,
            clip_rect: None,
        };
        self.child.paint(&mut sub_ctx);
        let picture = sub_rec.finish();

        ctx.attach_transform(TransformLayerEntry {
            picture,
            child_size,
            viewport_rect: vp,
            scroll_x: off[0],
            scroll_y: off[1],
        });

        // Wheel scrolling → offset channel (no repaint). Axis-clamped.
        let max_x = match self.axis {
            ScrollAxis::Horizontal | ScrollAxis::Both => (child_size.width - vp.size.width).max(0.0),
            ScrollAxis::Vertical => 0.0,
        };
        let max_y = match self.axis {
            ScrollAxis::Vertical | ScrollAxis::Both => (child_size.height - vp.size.height).max(0.0),
            ScrollAxis::Horizontal => 0.0,
        };
        let axes = match self.axis {
            ScrollAxis::Vertical   => super::ScrollAxes::Y,
            ScrollAxis::Horizontal => super::ScrollAxes::X,
            ScrollAxis::Both       => super::ScrollAxes::BOTH,
        };
        ctx.register_scroll_target(vp, axes, Arc::new(move |dx, dy| {
            rosace_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
        }));

        // Scrollbar drawn into the base canvas from the live channel offset.
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both)
            && child_size.height > vp.size.height
        {
            let ratio = (vp.size.height / child_size.height).min(1.0);
            let bar_h = vp.size.height * ratio;
            let bar_y = vp.origin.y + (off[1] / child_size.height) * vp.size.height;
            ctx.fill_rect(Rect {
                origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                size: Size { width: 3.0, height: bar_h },
            }, self.scrollbar_color);
        }
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both)
            && child_size.width > vp.size.width
        {
            let ratio = (vp.size.width / child_size.width).min(1.0);
            let bar_w = vp.size.width * ratio;
            let bar_x = vp.origin.x + (off[0] / child_size.width) * vp.size.width;
            ctx.fill_rect(Rect {
                origin: Point { x: bar_x, y: vp.origin.y + vp.size.height - 4.0 },
                size: Size { width: bar_w, height: 3.0 },
            }, self.scrollbar_color);
        }
    }

    /// Base (CPU-painted) path: content painted directly into the main
    /// canvas at the scroll offset, clipped to the viewport. `child_size` is
    /// measured once by the caller ([`Widget::paint`]) and passed in.
    fn paint_base(&self, ctx: &mut PaintCtx, child_size: Size) {
        let vp = ctx.rect;

        // Resolve the controller: explicit override, or the node's implicit
        // one (D101). Fixed mode has no controller and never handles input.
        let ctrl = if self.fixed_offset.is_some() {
            None
        } else {
            Some(self.controller.clone().unwrap_or_else(|| ctx.scroll_controller()))
        };

        let (scroll_x, scroll_y) = match (&ctrl, self.fixed_offset) {
            (Some(c), _) => {
                let [x, y] = c.offset.get();
                (x, y)
            }
            (None, Some(o)) => match self.axis {
                ScrollAxis::Horizontal => (o, 0.0),
                _ => (0.0, o),
            },
            (None, None) => (0.0, 0.0),
        };

        let (ox, oy) = match self.axis {
            ScrollAxis::Vertical   => (0.0, -scroll_y),
            ScrollAxis::Horizontal => (-scroll_x, 0.0),
            ScrollAxis::Both       => (-scroll_x, -scroll_y),
        };

        let child_rect = Rect {
            origin: Point { x: vp.origin.x + ox, y: vp.origin.y + oy },
            size: child_size,
        };

        // Clip child paint output to the viewport.
        ctx.record(DrawCommand::PushClip { rect: vp });
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, vp))
            .unwrap_or(vp);
        let mut child_ctx = ctx.child(child_rect);
        child_ctx.clip_rect = Some(effective_clip);
        self.child.paint(&mut child_ctx);
        ctx.record(DrawCommand::PopClip);

        // Publish extents (guarded — unconditional atom writes during paint
        // would dirty the component every frame) and route wheel input.
        if let Some(ctrl) = &ctrl {
            let vp_s = [vp.size.width, vp.size.height];
            if ctrl.viewport_size.get() != vp_s { ctrl.viewport_size.set(vp_s); }
            let cs = [child_size.width, child_size.height];
            if ctrl.content_size.get() != cs { ctrl.content_size.set(cs); }

            let axes = match self.axis {
                ScrollAxis::Vertical   => super::ScrollAxes::Y,
                ScrollAxis::Horizontal => super::ScrollAxes::X,
                ScrollAxis::Both       => super::ScrollAxes::BOTH,
            };
            let (ax, ay) = (axes.x, axes.y);

            let physics = resolve_physics(&ctx.theme, self.physics);

            // Drag-to-pan (D108/Phase 26 Step 2): streams absolute drag
            // position via the same positional-hit mechanism sliders use,
            // axis-clamped like wheel input above. Registering this also
            // makes the viewport a `hits_at` region, so `ctx.pressed()`
            // below picks it up for free via the same `hover_test` walk
            // Step 1's press state already resolves through.
            let drag_ctrl = ctrl.clone();
            ctx.on_press_at(move |x, y| {
                let (dx, dy) = drag_ctrl.drag_delta(x, y);
                // Content follows the finger: dragging up (dy < 0) reveals
                // what's below, i.e. INCREASES the offset — negate, exactly
                // like the wheel-scroll callback above already does.
                drag_ctrl.apply_momentum(if ax { -dx } else { 0.0 }, if ay { -dy } else { 0.0 }, physics);
            });

            // Momentum/bounce drive (D108/Phase 26 Step 2): tracks the REAL
            // drag speed while pressed, hands off to decay/spring-back once
            // released. Reuses Step 1's `pressed()` (same node, since the
            // `on_press_at` region declared above lands on this node).
            //
            // Deliberately does NOT reset `last_drag_point` on a was_pressed
            // false→true transition here — `ctx.pressed()` lags the real
            // MouseDown event by one frame (same as `ctx.hovered()`), so
            // that transition is observed on the SAME frame as the drag's
            // first `MouseMove`, one frame after `drag_delta`'s own
            // None-baseline logic already established the starting point
            // from MouseDown's immediate callback invocation. Resetting
            // here would wipe that baseline out from under the very next
            // `drag_delta` call — `end_drag` on release (below) is the only
            // reset needed; a fresh press always starts from `None` anyway
            // since release already cleared it.
            let dt = rosace_animate::frame_dt().max(0.0001);
            let is_pressed = ctx.pressed();
            let was_pressed = ctrl.was_pressed();
            // Whether a wheel/trackpad event landed recently (real elapsed
            // time, not "this exact frame") — if so, hold off `coast`'s
            // decay/spring-back. A single-frame version of this check (an
            // earlier revision) sprang back the instant one frame happened
            // to have no fresh wheel event, then got pushed forward again
            // by the next one, producing a visible jitter right at the
            // boundary (found via real trackpad testing — "vibration,
            // scroll a little up and down"). Real wheel events don't arrive
            // on a perfectly even one-per-frame cadence, so a short real
            // time grace period (`WHEEL_IDLE_GRACE`) is needed instead of a
            // single-frame flag. Also, without ANY such gate at all, `coast`
            // ran every frame wheel input was active (not just after it
            // stopped, since wheel scrolling never sets `pressed`) —
            // friction decayed the velocity away while the user was still
            // actively scrolling, so nothing real was left to coast with by
            // release.
            ctrl.advance_wheel_idle(dt);
            if is_pressed {
                ctrl.track_velocity(dt);
            } else if ctrl.wheel_recently_active() {
                ctx.request_animation(); // keep the loop alive so coast resumes once wheel events truly stop
            } else {
                if was_pressed { ctrl.end_drag(); }
                if !ctx.theme.animation.enabled {
                    ctrl.stop_coasting();
                } else if ctrl.coast(physics, dt) {
                    ctx.request_animation();
                }
            }
            ctrl.set_was_pressed(is_pressed);

            // Wheel/trackpad input applies its own delta directly (still
            // respecting Bounce's overscroll resistance via
            // `apply_momentum`) but does NOT inject a synthetic velocity
            // for `coast` to decay (D108/Phase 26 Step 2, revised after real
            // trackpad testing). Reasoning, confirmed by reading winit's own
            // macOS backend source, not assumed: a trackpad's "coast" feel
            // during and after a swipe is largely the OS's OWN native
            // momentum-phase event stream (`NSEvent.momentumPhase`) —
            // winit's `scrollWheel:` handler reads it and keeps sending
            // Scroll events for a while after fingers lift. Layering a
            // SECOND, app-level momentum system on top fought with that OS
            // tail: each native momentum-phase event nudged the offset
            // further, ROSACE's own spring-back tried to recover, the next
            // OS event pushed past the edge again — a real, reproducible
            // oscillation, confirmed frame-by-frame from a screen recording
            // (settled, then overscrolled again, then re-settled, well
            // after release). winit collapses BOTH real finger movement and
            // OS momentum-phase events into the same `TouchPhase::Moved` —
            // there's no reliable way to tell them apart from the event
            // alone, so the only robust fix is to not double up: ROSACE's
            // own velocity-tracked momentum is reserved for drag gestures
            // (mouse/touch press-drag-release), which have no OS-native
            // momentum layer to conflict with. Once wheel input goes idle
            // (`wheel_recently_active` false), `coast`'s Bounce-overscroll
            // check (checked first, independent of velocity) still springs
            // back if left showing blank space — so overscroll recovery
            // still works, it just isn't fighting a second momentum source.
            // Honest limitation: a plain (non-trackpad) mouse wheel has no
            // OS-native momentum either, so it also won't coast under this
            // scheme — distinguishing that case needs LineDelta/PixelDelta
            // and momentum-phase info threaded through
            // `rosace_platform::InputEvent::Scroll`, which doesn't carry
            // it today; flagged as real follow-up, not silently claimed.
            let wheel_ctrl = ctrl.clone();
            ctx.register_scroll_target(vp, axes, Arc::new(move |dx, dy| {
                let ddx = if ax { -dx } else { 0.0 };
                let ddy = if ay { -dy } else { 0.0 };
                wheel_ctrl.apply_momentum(ddx, ddy, physics);
                wheel_ctrl.mark_wheel_active();
            }));
        }

        // Scrollbars drawn AFTER PopClip so they are not clipped. Re-reads
        // the offset fresh here (D108/Phase 26 Step 2) rather than reusing
        // `scroll_x`/`scroll_y` captured at the top of this function —
        // those predate this frame's drag/wheel/momentum updates further
        // above, so the thumb would lag a full frame behind the content
        // it's supposed to track (most visible during a fast momentum
        // coast, where a frame's movement is largest).
        let (fresh_x, fresh_y) = match &ctrl {
            Some(c) => { let [x, y] = c.offset.get(); (x, y) }
            None => (scroll_x, scroll_y),
        };
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both) {
            let ratio = (vp.size.height / child_size.height.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_h = vp.size.height * ratio;
                // Clamp the THUMB's visible position to the track — under
                // `Bounce`, `fresh_y` can go negative or past the max
                // during an overscroll, which without this would push the
                // thumb (or its rect) off the visible track entirely,
                // looking like the scrollbar "isn't responding" (found via
                // real trackpad testing). The content itself still tracks
                // the real (unclamped) offset; only the thumb's on-screen
                // position is clamped.
                let max_bar_y = vp.origin.y + vp.size.height - bar_h;
                let bar_y = (vp.origin.y + (fresh_y / child_size.height) * vp.size.height)
                    .clamp(vp.origin.y, max_bar_y.max(vp.origin.y));
                ctx.fill_rect(Rect {
                    origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                    size: Size { width: 3.0, height: bar_h },
                }, self.scrollbar_color);
            }
        }
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both) {
            let ratio = (vp.size.width / child_size.width.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_w = vp.size.width * ratio;
                let max_bar_x = vp.origin.x + vp.size.width - bar_w;
                let bar_x = (vp.origin.x + (fresh_x / child_size.width) * vp.size.width)
                    .clamp(vp.origin.x, max_bar_x.max(vp.origin.x));
                ctx.fill_rect(Rect {
                    origin: Point { x: bar_x, y: vp.origin.y + vp.size.height - 4.0 },
                    size: Size { width: bar_w, height: 3.0 },
                }, self.scrollbar_color);
            }
        }
    }
}

impl Widget for ScrollView {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;
        let child_size = self.child.layout(&ctx.layout_ctx(self.child_constraints(vp)));

        // `::fixed` and `::controlled` always use the base path — exact,
        // un-composited semantics for programmatic control and snapshots.
        // Otherwise: explicit `.gpu_layer()` forces the GPU path on; plain
        // `ScrollView::new` auto-detects it via the size heuristic (D090
        // transparent default).
        let eligible = self.fixed_offset.is_none() && self.controller.is_none();
        let use_gpu = eligible
            && (self.gpu_layer || self.should_auto_gpu(vp.size, child_size));

        if use_gpu {
            self.paint_gpu(ctx, child_size);
        } else {
            self.paint_base(ctx, child_size);
        }
    }
}
