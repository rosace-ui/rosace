use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::{Color, DrawCommand};
use crate::scroll::{ScrollController, ScrollPhysics, ScrollStyle};
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
/// Deprecated alias — the cap now lives with the layer it constrains.
/// Kept so existing callers keep compiling.
pub const MAX_TL_DIM: f32 = super::transform_layer::MAX_TRANSFORM_DIM as f32;

/// How strongly the `Bounce` spring recovers WHILE wheel/trackpad momentum
/// events are still arriving (as opposed to full-strength once they've
/// truly gone idle) — a fraction applied to `dt` before calling
/// `settle_bounce`, not a separate physics constant, so it reuses the exact
/// same spring math just running "in slow motion" relative to real time.
/// 0.15 was chosen empirically (real trackpad testing) to sit comfortably
/// below the pull each individual resisted wheel push contributes (`bounce_
/// axis` already resists those to 35% of their raw delta) — high enough to
/// visibly close most of the gap before the events truly stop (cutting the
/// old unbounded freeze down to a brief, subtle glide), low enough that the
/// two don't visibly fight each other frame-to-frame (full strength here
/// oscillated: push out, spring back further, push out again).
const CONCURRENT_BOUNCE_DT_SCALE: f32 = 0.15;

/// When the scrollbar thumb/track is drawn at all (D-SCROLLBAR-1 — user-
/// reported: during a screen-transition slide, an always-drawn thumb reads
/// as a small opaque box detached from the rest of the sliding UI; fading
/// it away when idle/off-screen sidesteps that entirely, not just on
/// transitions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarVisibility {
    /// Always visible whenever there's overflow to scroll.
    #[default]
    Always,
    /// Hidden until a drag, wheel, or momentum-coast gesture is active;
    /// fades out shortly after the content settles.
    WhileScrolling,
    /// Visible while the pointer hovers the track, OR while actively
    /// scrolling (falls back to `WhileScrolling`'s behavior with no mouse
    /// — touch/mobile has no hover to trigger on).
    OnHover,
    /// Never drawn — same effect as [`ScrollView::no_scrollbar`].
    Hidden,
}

/// Full scrollbar appearance + behavior, set with [`ScrollView::scrollbar_style`].
/// The individual `.scrollbar_color()`/`.no_scrollbar()` shorthands still work
/// and just edit this struct's fields.
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarStyle {
    pub visibility: ScrollbarVisibility,
    /// Thumb fill.
    /// `None` = the active theme's `outline`, resolved at paint time. A
    /// fixed default here meant the one colour this widget owns ignored the
    /// theme, and was a dark-theme blue-grey in a light app.
    pub color: Option<Color>,
    /// Track background drawn behind the thumb along the whole scrollable
    /// edge. `None` (default) draws no track, matching the previous
    /// thumb-only look.
    pub track_color: Option<Color>,
    /// Thumb (and track) thickness in logical px.
    pub thickness: f32,
    /// Corner radius — `0.0` for the previous square-cornered look.
    pub radius: f32,
    /// Gap between the thumb and the viewport's far edge.
    pub inset: f32,
    /// Floor on the thumb's drawn length so a huge content/viewport ratio
    /// never shrinks it down to an unclickable sliver.
    pub min_thumb_length: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::Always,
            color: None,
            track_color: None,
            thickness: 3.0,
            radius: 1.5,
            inset: 4.0,
            min_thumb_length: 24.0,
        }
    }
}

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
    /// `None` = no inset — content is flush to the viewport.
    padding: Option<super::EdgeInsets>,
    child: BoxedWidget,
    /// Fixed offset for [`ScrollView::fixed`] snapshot mode.
    fixed_offset: Option<f32>,
    /// Explicit controller override (D101). `None` = implicit node controller.
    controller: Option<ScrollController>,
    pub axis: ScrollAxis,
    pub scrollbar: ScrollbarStyle,
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
            padding: None,
            child: Box::new(child),
            fixed_offset: None,
            controller: None,
            axis: ScrollAxis::Vertical,
            scrollbar: ScrollbarStyle::default(),
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
    pub fn no_scrollbar(mut self) -> Self { self.scrollbar.visibility = ScrollbarVisibility::Hidden; self }
    pub fn scrollbar_color(mut self, c: Color) -> Self { self.scrollbar.color = Some(c); self }

    /// Inset the scrolled CONTENT without insetting the viewport, so the
    /// scrollbar still tracks the full height and content does not sit
    /// under it. Previously impossible: the child was laid out flush.
    pub fn padding(mut self, p: super::EdgeInsets) -> Self { self.padding = Some(p); self }
    /// Full scrollbar style (visibility mode, color, track, thickness,
    /// radius, inset, minimum thumb length) in one call.
    pub fn scrollbar_style(mut self, s: ScrollbarStyle) -> Self { self.scrollbar = s; self }
    /// Just the visibility mode — shorthand for `.scrollbar_style(..)` when
    /// only that needs to change.
    pub fn scrollbar_visibility(mut self, v: ScrollbarVisibility) -> Self { self.scrollbar.visibility = v; self }

    /// Content constraints (unbounded-axis doctrine, API_DESIGN §6): on the
    /// scroll axis min = viewport, max = Unbounded. Shared by both the GPU
    /// and base paint paths so content is measured identically either way.
    fn child_constraints(&self, vp: Rect) -> Constraints {
        use rosace_layout::AxisBound;
        // Content padding narrows the CHILD without narrowing the viewport,
        // so the scrollbar still tracks the full height and content does not
        // sit underneath it.
        let pad = self.padding.unwrap_or_default();
        let vp = Rect {
            origin: vp.origin,
            size: Size {
                width: (vp.size.width - pad.total_h()).max(0.0),
                height: (vp.size.height - pad.total_v()).max(0.0),
            },
        };
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
        // PHYSICAL fit: the offscreen texture is allocated at `extent * scale`
        // and hard-capped at `MAX_TL_DIM` (engine.rs). A logical-only check
        // (`extent <= MAX_TL_DIM`) passes content that then can't fit its
        // texture on a 2x/3x display, clipping the bottom. Gate on the physical
        // size so taller-than-cap content falls to the CPU (base) path — which
        // re-renders only the visible slice and has no single-texture limit.
        overflow && extent * rosace_state::render_scale() <= MAX_TL_DIM
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

        // Controller-backed offset (D101) — the SAME model `paint_base` uses,
        // so the GPU path gets real drag + flick momentum instead of wheel
        // only. This path composites the content as an offscreen texture and
        // shifts its sample offset each frame, so the live offset is also
        // mirrored to the non-reactive channel the compositor reads
        // (`scroll_offset`): the controller is the source of truth.
        let ctrl = ctx.scroll_controller();
        let axes = match self.axis {
            ScrollAxis::Vertical   => super::ScrollAxes::Y,
            ScrollAxis::Horizontal => super::ScrollAxes::X,
            ScrollAxis::Both       => super::ScrollAxes::BOTH,
        };
        let (ax, ay) = (axes.x, axes.y);
        let physics = resolve_physics(&ctx.theme, self.physics);

        // Publish extents so `apply_momentum`/`coast` can clamp (guarded — an
        // unconditional atom write during paint would dirty every frame).
        let vp_s = [vp.size.width, vp.size.height];
        if ctrl.viewport_size.get() != vp_s { ctrl.viewport_size.set(vp_s); }
        let cs = [child_size.width, child_size.height];
        if ctrl.content_size.get() != cs { ctrl.content_size.set(cs); }

        // Momentum drive — identical to `paint_base`: track drag velocity
        // while pressed, coast / spring-back once released (unless wheel input
        // is still live). See `paint_base` for the wheel-idle-grace rationale.
        let dt = rosace_animate::frame_dt().max(0.0001);
        let is_pressed = ctx.pressed();
        let was_pressed = ctrl.was_pressed();
        ctrl.advance_wheel_idle(dt);
        if is_pressed {
            ctrl.track_velocity(dt);
        } else if ctrl.wheel_recently_active() {
            // A `Bounce` spring must keep recovering even while the OS's
            // native momentum-phase wheel events are still arriving — see
            // the long comment on this same branch in `paint_base` for why
            // waiting for them to stop first produced a visible "pause,
            // then snap back" that grew with flick speed. Heavily damped
            // (`CONCURRENT_BOUNCE_DT_SCALE`) — see that constant's own doc
            // comment for why a full-strength spring here visibly vibrated.
            if let ScrollPhysics::Bounce { spring_stiffness, .. } = physics {
                if ctrl.is_overscrolled() {
                    ctrl.settle_bounce(spring_stiffness, dt * CONCURRENT_BOUNCE_DT_SCALE);
                }
            }
            ctx.request_animation();
        } else {
            if was_pressed { ctrl.end_drag(); }
            if !ctx.theme.animation.enabled {
                ctrl.stop_coasting();
            } else if ctrl.coast(physics, dt) {
                ctx.request_animation();
            }
        }
        ctrl.set_was_pressed(is_pressed);

        // Live (post-coast) offset drives BOTH this frame's transform and the
        // compositor's offscreen sample position (via the mirrored channel).
        let off = ctrl.offset.get();
        rosace_state::set_scroll_offset(node_id, off);

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
            zoom: 1.0,
            scroll_x: off[0],
            scroll_y: off[1],
        });

        // Wheel/trackpad → `apply_momentum` (respects Bounce overscroll),
        // marks wheel active so coast holds off while it's live.
        let wheel_ctrl = ctrl.clone();
        ctx.register_scroll_target(vp, axes, Arc::new(move |dx, dy| {
            wheel_ctrl.apply_momentum(if ax { -dx } else { 0.0 }, if ay { -dy } else { 0.0 }, physics);
            wheel_ctrl.mark_wheel_active();
        }));

        // Touch/mouse drag-to-pan (GPU-path parity): a finger produces no
        // wheel event, so without this the GPU scroll path could not scroll on
        // touch devices at all (the gallery was frozen on iOS, fine on the
        // Mac trackpad). Nested-scroll-chain-aware (D-NESTED-SCROLL,
        // 2026-08-02) — see the base path's own registration for why this
        // is `register_nested_scroll`, not `on_press_at`.
        let pan_ctrl = ctrl.clone();
        ctx.register_nested_scroll(move |dx, dy| {
            pan_ctrl.try_apply_delta(if ax { -dx } else { 0.0 }, if ay { -dy } else { 0.0 }, physics)
        });

        // Scrollbar drawn into the base canvas from the live channel offset.
        self.draw_scrollbars(ctx, vp, child_size, off, Some(&ctrl), is_pressed);
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

        let pad = self.padding.unwrap_or_default();
        let child_rect = Rect {
            origin: Point {
                x: vp.origin.x + ox + pad.left,
                y: vp.origin.y + oy + pad.top,
            },
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

            // Drag-to-pan (D108/Phase 26 Step 2; nested-scroll-chain-aware
            // since D-NESTED-SCROLL, 2026-08-02): a `ScrollHandler` link
            // via `register_nested_scroll`, not the flat always-consumes
            // `on_press_at` sliders use — reports whether the delta
            // actually moved the offset, so once this view is exhausted
            // in the drag's direction (hard-clamped, or stretched to its
            // own `Bounce` limit), the SAME delta falls through to
            // whatever scrollable ancestor encloses it, instead of the
            // gesture just silently doing nothing. Registering this also
            // makes the viewport a `nested_scrolls` region, so
            // `ctx.pressed()` below picks it up for free via the same
            // `hover_test` walk Step 1's press state already resolves
            // through (`hover_test_node` checks `nested_scrolls` too).
            let drag_ctrl = ctrl.clone();
            ctx.register_nested_scroll(move |dx, dy| {
                // Content follows the finger: dragging up (dy < 0) reveals
                // what's below, i.e. INCREASES the offset — negate, exactly
                // like the wheel-scroll callback above already does.
                drag_ctrl.try_apply_delta(if ax { -dx } else { 0.0 }, if ay { -dy } else { 0.0 }, physics)
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
                // Real trackpad testing (2026-08-01): a fast flick's native
                // momentum-phase wheel-event tail can run for a while — this
                // branch stays active that whole time, and previously did
                // nothing but wait, so an ALREADY-overscrolled `Bounce` view
                // sat frozen at the rubber-band limit until the OS finally
                // stopped sending events, then sprang back — a pause whose
                // length scaled directly with flick speed (longer flick =
                // longer native momentum tail = longer freeze). The spring
                // must keep recovering concurrently with those still-
                // arriving events, not wait for them to end; the wheel
                // callback's own `apply_momentum` (via `bounce_axis`) still
                // resists any further push deeper into overscroll, so this
                // doesn't fight it, it just lets the recoil run at the same
                // time — matching real trackpad/UIScrollView feel, where you
                // can feel resistance AND a slight recoil simultaneously.
                // Heavily damped (`CONCURRENT_BOUNCE_DT_SCALE`) — a full-
                // strength spring here fought each still-arriving resisted
                // push hard enough to visibly vibrate (real trackpad
                // testing, 2026-08-01 follow-up): push out 35%-resisted,
                // spring pulls back a large fraction of that same distance,
                // next event pushes again — a sawtooth. Damping the spring's
                // own effective time step keeps its pull well below what a
                // single resisted push contributes, so it net-decays smoothly
                // toward the bound instead of visibly fighting each event.
                if let ScrollPhysics::Bounce { spring_stiffness, .. } = physics {
                    if ctrl.is_overscrolled() {
                        ctrl.settle_bounce(spring_stiffness, dt * CONCURRENT_BOUNCE_DT_SCALE);
                    }
                }
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
        let fresh = match &ctrl {
            Some(c) => c.offset.get(),
            None => [scroll_x, scroll_y],
        };
        self.draw_scrollbars(ctx, vp, child_size, fresh, ctrl.as_ref(), ctx.pressed());
    }

    /// Shared by both the GPU and base paint paths — draws the thumb (and
    /// optional track), governed by `self.scrollbar`'s visibility mode.
    /// `off` is this frame's freshly-read scroll offset (not a value
    /// captured earlier in the same paint call — see the callers' own
    /// comments on why "fresh" matters for `Bounce` overscroll).
    fn draw_scrollbars(
        &self,
        ctx: &mut PaintCtx,
        vp: Rect,
        child_size: Size,
        off: [f32; 2],
        ctrl: Option<&ScrollController>,
        is_pressed: bool,
    ) {
        let st = &self.scrollbar;
        if st.visibility == ScrollbarVisibility::Hidden {
            return;
        }

        let show_v = matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both)
            && child_size.height > vp.size.height.max(1.0);
        let show_h = matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both)
            && child_size.width > vp.size.width.max(1.0);
        if !show_v && !show_h {
            return;
        }

        // A generous strip along the scrollable edge, not just the exact
        // thumb rect — real scrollbars reveal on hovering anywhere near the
        // edge, not only when the cursor lands precisely on the (possibly
        // short) thumb.
        const HOVER_STRIP: f32 = 14.0;
        let (px, py) = super::current_pointer();
        let in_rect = |r: Rect| px >= r.origin.x && px <= r.origin.x + r.size.width
            && py >= r.origin.y && py <= r.origin.y + r.size.height;

        let mut v_thumb = None;
        if show_v {
            let ratio = (vp.size.height / child_size.height.max(1.0)).min(1.0);
            let bar_h = (vp.size.height * ratio).max(st.min_thumb_length);
            // Clamp the THUMB's visible position to the track — under
            // `Bounce`, `off[1]` can go negative or past the max during an
            // overscroll, which without this would push the thumb off the
            // visible track entirely, looking like the scrollbar "isn't
            // responding" (found via real trackpad testing). The content
            // itself still tracks the real (unclamped) offset; only the
            // thumb's on-screen position is clamped.
            let max_bar_y = vp.origin.y + vp.size.height - bar_h;
            let bar_y = (vp.origin.y + (off[1] / child_size.height) * vp.size.height)
                .clamp(vp.origin.y, max_bar_y.max(vp.origin.y));
            let bar_x = vp.origin.x + vp.size.width - st.inset - st.thickness;
            v_thumb = Some(Rect {
                origin: Point { x: bar_x, y: bar_y },
                size: Size { width: st.thickness, height: bar_h },
            });
        }
        let mut h_thumb = None;
        if show_h {
            let ratio = (vp.size.width / child_size.width.max(1.0)).min(1.0);
            let bar_w = (vp.size.width * ratio).max(st.min_thumb_length);
            let max_bar_x = vp.origin.x + vp.size.width - bar_w;
            let bar_x = (vp.origin.x + (off[0] / child_size.width) * vp.size.width)
                .clamp(vp.origin.x, max_bar_x.max(vp.origin.x));
            let bar_y = vp.origin.y + vp.size.height - st.inset - st.thickness;
            h_thumb = Some(Rect {
                origin: Point { x: bar_x, y: bar_y },
                size: Size { width: bar_w, height: st.thickness },
            });
        }

        let hovered = st.visibility == ScrollbarVisibility::OnHover && {
            let v_strip = show_v.then_some(Rect {
                origin: Point { x: vp.origin.x + vp.size.width - st.inset - st.thickness - HOVER_STRIP, y: vp.origin.y },
                size: Size { width: st.thickness + st.inset + HOVER_STRIP, height: vp.size.height },
            });
            let h_strip = show_h.then_some(Rect {
                origin: Point { x: vp.origin.x, y: vp.origin.y + vp.size.height - st.inset - st.thickness - HOVER_STRIP },
                size: Size { width: vp.size.width, height: st.thickness + st.inset + HOVER_STRIP },
            });
            v_strip.is_some_and(in_rect) || h_strip.is_some_and(in_rect)
        };
        let active = is_pressed
            || ctrl.is_some_and(|c| c.wheel_recently_active()
                || c.velocity_magnitude() > crate::scroll::controller::COAST_STOP_THRESHOLD);

        let target = match st.visibility {
            ScrollbarVisibility::Hidden => 0.0,
            ScrollbarVisibility::Always => 1.0,
            ScrollbarVisibility::WhileScrolling => if active { 1.0 } else { 0.0 },
            ScrollbarVisibility::OnHover => if hovered || active { 1.0 } else { 0.0 },
        };
        // Channel 0 — nothing else on a ScrollView's own node animates
        // today, but reserved via `animate_channel` (not `animate_to`) so a
        // future per-node animation here doesn't silently collide.
        let opacity = ctx.animate_channel(0, target, 0.0);
        if opacity <= 0.001 {
            return;
        }
        let with_alpha = |c: Color| Color::rgba(c.r, c.g, c.b, (c.a as f32 * opacity).round() as u8);

        if let Some(track_color) = st.track_color {
            if let Some(r) = v_thumb {
                let track = Rect {
                    origin: Point { x: r.origin.x, y: vp.origin.y },
                    size: Size { width: st.thickness, height: vp.size.height },
                };
                ctx.fill_rrect(track, st.radius, with_alpha(track_color));
            }
            if let Some(r) = h_thumb {
                let track = Rect {
                    origin: Point { x: vp.origin.x, y: r.origin.y },
                    size: Size { width: vp.size.width, height: st.thickness },
                };
                ctx.fill_rrect(track, st.radius, with_alpha(track_color));
            }
        }
        let thumb_col = st.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.outline));
        if let Some(r) = v_thumb { ctx.fill_rrect(r, st.radius, with_alpha(thumb_col)); }
        if let Some(r) = h_thumb { ctx.fill_rrect(r, st.radius, with_alpha(thumb_col)); }
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
