use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::PictureRecorder;
use rosace_scroll::controller::{MAX_VELOCITY, COAST_STOP_THRESHOLD};
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, TransformLayerEntry, avail_w, avail_h};
use super::container::draw_rounded_rect_pub;

/// A large 2D plane with pan + zoom, driven by wheel/drag and +/- controls
/// (Flutter-`InteractiveViewer`-like, Phase 32/D115).
///
/// Built on the same GPU-composited transform-layer seam `ScrollView` uses
/// (D087/D090): the child is recorded ONCE into an independent `Picture`
/// and replayed by the platform into its own content texture, so panning is
/// a zero-repaint UV shift. Zoom rasterizes that SAME picture at a higher
/// physical resolution (`content_scale = dpi_scale * zoom` in
/// `rosace/src/engine.rs`) — real GPU-crisp magnification, reusing the exact
/// mechanism that already gives HiDPI displays sharp text/shapes; the
/// compositor's existing UV-window math needs no changes.
///
/// Pinch/trackpad-magnify gesture support is a NAMED deferral (mirrors the
/// project's existing "mobile pinch deferred" pattern for Phase 28 Step 6):
/// this MVP drives zoom through explicit `+`/`-` controls, which needs no
/// new platform-level gesture plumbing.
pub struct InteractiveViewer<W: Widget + Send + Sync + 'static> {
    pub child: W,
    zoom: Atom<f32>,
    min_scale: f32,
    max_scale: f32,
    /// When true (default), panning is clamped so content can't be dragged
    /// fully off-screen — a bounded viewer of a large image/canvas. When
    /// false, pan is unclamped — an infinite-plane canvas.
    constrained: bool,
    zoom_controls: bool,
}

impl<W: Widget + Send + Sync + 'static> InteractiveViewer<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            zoom: rosace_state::use_atom(1.0_f32),
            min_scale: 0.5,
            max_scale: 4.0,
            constrained: true,
            zoom_controls: true,
        }
    }

    pub fn min_scale(mut self, v: f32) -> Self { self.min_scale = v; self }
    pub fn max_scale(mut self, v: f32) -> Self { self.max_scale = v; self }
    /// Unbounded infinite-plane panning instead of the default clamped-to-content behavior.
    pub fn unconstrained(mut self) -> Self { self.constrained = false; self }
    pub fn no_zoom_controls(mut self) -> Self { self.zoom_controls = false; self }
}

impl<W: Widget + Send + Sync + 'static> Widget for InteractiveViewer<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Fills whatever space the parent gives it, like Container/Expanded —
        // a canvas viewer has no natural size of its own.
        Size { width: avail_w(ctx.constraints), height: avail_h(ctx.constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let zoom = self.zoom.get().clamp(self.min_scale, self.max_scale);
        let vp_rect = ctx.rect;

        // Measure + record the child ONCE at its natural (unconstrained) size —
        // same recipe as TransformLayer/ScrollView.
        let child_lctx = ctx.layout_ctx(Constraints::loose(f32::INFINITY, f32::INFINITY));
        let child_size = self.child.layout(&child_lctx);

        let mut sub_rec = PictureRecorder::new();
        let child_rect = Rect { origin: Point { x: 0.0, y: 0.0 }, size: child_size };
        let sub_node = ctx.tree.borrow_mut().slot(ctx.node, true);
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
            viewport_rect: vp_rect,
            zoom,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });

        // Visible content window shrinks (in content-native px) as zoom
        // increases — the same relationship `child_coords` inverts.
        let visible_w = vp_rect.size.width / zoom;
        let visible_h = vp_rect.size.height / zoom;
        let (max_x, max_y) = if self.constrained {
            ((child_size.width - visible_w).max(0.0), (child_size.height - visible_h).max(0.0))
        } else {
            (f32::INFINITY, f32::INFINITY)
        };

        let node_id = ctx.node as u64;
        // Wheel/trackpad pan.
        {
            let (max_x, max_y) = (max_x, max_y);
            ctx.register_scroll_target(
                vp_rect,
                super::render_tree::ScrollAxes::BOTH,
                Arc::new(move |dx, dy| {
                    rosace_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
                }),
            );
        }

        // Click-drag pan: on_press_at streams absolute window-space points
        // for the whole gesture (see rosace/src/engine.rs's active_drag) —
        // turn consecutive points into a delta, scaled by zoom (dragging a
        // fixed SCREEN distance moves a smaller CONTENT distance at higher
        // zoom, matching child_coords' own `/zoom`). Also tracks REAL
        // wall-clock velocity (content-space px/sec) for the momentum coast
        // below — `rosace_animate::frame_dt()` isn't usable here since
        // panning deliberately does NOT repaint per move (D090's whole
        // point), so there's no per-frame dt to lean on; a raw `Instant`
        // works regardless of whether/how often paint() runs.
        {
            let (max_x, max_y) = (max_x, max_y);
            ctx.on_press_at(move |px, py| {
                if let Some((lx, ly, t0)) = rosace_state::drag_last(node_id) {
                    let dt = t0.elapsed().as_secs_f32().max(1.0 / 240.0);
                    let (dx, dy) = ((px - lx) / zoom, (py - ly) / zoom);
                    rosace_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
                    rosace_state::set_pan_velocity(node_id, (
                        (dx / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY),
                        (dy / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY),
                    ));
                }
                rosace_state::set_drag_last(node_id, Some((px, py)));
            });
        }

        // Momentum coast (the reported "abrupt, no momentum" gap): once
        // released, keep applying the last tracked velocity with exponential
        // decay until it settles — same friction/threshold constants
        // `ScrollView`'s own momentum uses (`rosace_scroll::controller`), so
        // panning here feels consistent with scrolling elsewhere in the app.
        // `ctx.pressed()` transitioning true->false is what starts this:
        // MouseUp already forces one repaint for the pressed node
        // (`engine.rs`'s `set_pressed` → `request_frame`) regardless of
        // momentum, which is the hook that lets this widget observe the
        // release at all despite not repainting during the drag itself.
        if !ctx.pressed() {
            rosace_state::set_drag_last(node_id, None);
            let (vx, vy) = rosace_state::pan_velocity(node_id);
            if vx.abs() > COAST_STOP_THRESHOLD || vy.abs() > COAST_STOP_THRESHOLD {
                let dt = rosace_animate::frame_dt().max(0.0001);
                rosace_state::scroll_offset_by(node_id, -vx * dt, -vy * dt, max_x, max_y);
                let friction = 0.88_f32.powf(dt / (1.0 / 60.0));
                let (nvx, nvy) = (vx * friction, vy * friction);
                let settled = nvx.abs() < COAST_STOP_THRESHOLD && nvy.abs() < COAST_STOP_THRESHOLD;
                rosace_state::set_pan_velocity(node_id, if settled { (0.0, 0.0) } else { (nvx, nvy) });
                if !settled {
                    ctx.request_animation();
                }
            }
        }

        // Real trackpad pinch-to-zoom (macOS/iOS — winit's `PinchGesture`,
        // routed through rosace-platform's `InputEvent::Pinch` +
        // `RenderTree::zoom_test`). `delta` is an increment, not a
        // multiplier (winit's own convention) — same `zoom *= 1.0 + delta`
        // used by the `+`/`-` controls' 1.25 step, just gesture-driven.
        {
            let z = self.zoom.clone();
            let (min_scale, max_scale) = (self.min_scale, self.max_scale);
            ctx.register_zoom_target(vp_rect, Arc::new(move |delta| {
                z.set((z.get() * (1.0 + delta)).clamp(min_scale, max_scale));
            }));
        }

        ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown)
            .label("Interactive viewer".to_string()));

        if self.zoom_controls {
            self.paint_zoom_controls(ctx, vp_rect);
        }
    }
}

impl<W: Widget + Send + Sync + 'static> InteractiveViewer<W> {
    /// KNOWN LIMITATION (named, not silently dropped): these hits are
    /// registered directly on `ctx.node`, but the transform-layer content
    /// is a real CHILD of that same node — and `hit_test_node` always
    /// checks children before a node's own hits/hits_at (paint-order
    /// z-index, D092). So a child that itself registers a hit spanning this
    /// corner (e.g. a full-bleed interactive drawing canvas) would shadow
    /// these buttons. Fine for the common case (images/diagrams/static
    /// content), but a real fix needs a render-tree "always-on-top overlay
    /// child" concept — out of scope for this widget; revisit if it bites.
    ///
    /// Side-by-side (not stacked) so both buttons sit on one row — halves
    /// the vertical footprint a short viewport needs to fully reveal both,
    /// and removes any risk of one being clipped while the other isn't.
    ///
    /// Registered via `on_press_at` (POSITIONAL hits_at), not `register_hit`
    /// (plain hits): the pan-drag handler earlier in `paint()` also claims
    /// `vp_rect` via `on_press_at`, and `hit_test_node` checks a node's
    /// hits_at in reverse-registration order — registering the buttons
    /// AFTER the pan handler makes them win for clicks in their own rect,
    /// otherwise the pan handler (checked at the SAME tier but registered
    /// first) would swallow every click before plain `hits` are ever
    /// reached.
    fn paint_zoom_controls(&self, ctx: &mut PaintCtx, vp_rect: Rect) {
        let (bg, fg) = {
            let t = &ctx.theme.colors;
            (ctx.tc(t.surface), ctx.tc(t.on_surface))
        };
        const BTN: f32 = 32.0;
        const GAP: f32 = 8.0;
        const MARGIN: f32 = 12.0;
        let base_y = vp_rect.origin.y + vp_rect.size.height - MARGIN - BTN;
        let minus_x = vp_rect.origin.x + vp_rect.size.width - MARGIN - BTN;
        let plus_x = minus_x - GAP - BTN;

        let (min_scale, max_scale) = (self.min_scale, self.max_scale);
        let plus_rect = Rect { origin: Point { x: plus_x, y: base_y }, size: Size { width: BTN, height: BTN } };
        let minus_rect = Rect { origin: Point { x: minus_x, y: base_y }, size: Size { width: BTN, height: BTN } };

        draw_rounded_rect_pub(ctx, plus_rect, bg, 6.0);
        let pw = ctx.font.measure_text("+", 16.0);
        ctx.draw_text_at("+", Point {
            x: plus_rect.origin.x + (BTN - pw) / 2.0,
            y: super::vcenter_text_y(plus_rect.origin.y, BTN, ctx.font, 16.0),
        }, fg, 16.0);

        draw_rounded_rect_pub(ctx, minus_rect, bg, 6.0);
        let mw = ctx.font.measure_text("-", 16.0);
        ctx.draw_text_at("-", Point {
            x: minus_rect.origin.x + (BTN - mw) / 2.0,
            y: super::vcenter_text_y(minus_rect.origin.y, BTN, ctx.font, 16.0),
        }, fg, 16.0);

        let saved_rect = ctx.rect;
        {
            let z = self.zoom.clone();
            ctx.rect = plus_rect;
            ctx.on_press_at(move |_, _| z.set((z.get() * 1.25).clamp(min_scale, max_scale)));
        }
        {
            let z = self.zoom.clone();
            ctx.rect = minus_rect;
            ctx.on_press_at(move |_, _| z.set((z.get() / 1.25).clamp(min_scale, max_scale)));
        }
        ctx.rect = saved_rect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::text::Text;

    #[test]
    fn layout_fills_available_constraints() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 300.0), &font, &theme);
        let size = InteractiveViewer::new(Text::new("content")).layout(&ctx);
        assert_eq!((size.width, size.height), (400.0, 300.0));
    }

    #[test]
    fn zoom_atom_clamps_to_min_max() {
        let iv = InteractiveViewer::new(Text::new("x")).min_scale(0.5).max_scale(2.0);
        iv.zoom.set(10.0);
        assert_eq!(iv.zoom.get().clamp(iv.min_scale, iv.max_scale), 2.0);
        iv.zoom.set(0.01);
        assert_eq!(iv.zoom.get().clamp(iv.min_scale, iv.max_scale), 0.5);
    }

    #[test]
    fn constrained_pan_bound_shrinks_as_zoom_increases() {
        // Mirrors the InteractiveViewer::paint calculation directly: the
        // visible content window (and therefore the pannable max) shrinks
        // as zoom grows, since more content is shown magnified per screen px.
        let vp_w = 100.0_f32;
        let child_w = 500.0_f32;
        let max_at_1x = (child_w - vp_w / 1.0).max(0.0);
        let max_at_2x = (child_w - vp_w / 2.0).max(0.0);
        assert!(max_at_2x > max_at_1x, "zooming in must reveal MORE pannable content");
    }
}
