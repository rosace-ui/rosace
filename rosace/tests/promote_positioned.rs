//! `promote_at` — the overlay-shaped promotion: positioned against the window,
//! optionally behind a scrim.
//!
//! Every behaviour asserted here is one a real overlay depends on, and most are
//! bugs someone already hit: a `Fill` overlay laid out loose collapsed to its
//! child and stole clicks at the top-left; an anchored menu rendered off-screen
//! without the clamp; a modal that let clicks through to the page under it is
//! not modal; and a dropdown whose scrim did not exempt its own trigger fired
//! dismiss-and-reopen on a single click, so it could never be closed by the
//! control that opened it.

use rosace::prelude::*;
use rosace::widgets::tree::{
    FocusBehavior, InputBehavior, LayerPosition, LayoutCtx, PaintCtx, PromoteOpts, ScrimConfig,
};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const WIN_W: u32 = 300;
const WIN_H: u32 = 400;

/// Fixed-size overlay content that counts its own clicks.
struct Panel(Arc<AtomicUsize>);
impl Widget for Panel {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 100.0, height: 50.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(200, 40, 40));
        let hits = Arc::clone(&self.0);
        ctx.register_hit(Arc::new(move || { hits.fetch_add(1, Ordering::SeqCst); }));
    }
}

/// The widget under the overlay — anything reaching it means input leaked.
struct Beneath(Arc<AtomicUsize>);
impl Widget for Beneath {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: 200.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(20, 60, 20));
        let hits = Arc::clone(&self.0);
        ctx.register_hit(Arc::new(move || { hits.fetch_add(1, Ordering::SeqCst); }));
    }
}

struct Spec {
    position: LayerPosition,
    scrim: bool,
    exclude: Option<Rect>,
    input: InputBehavior,
    focus: FocusBehavior,
}

struct Host {
    spec: Arc<Mutex<Spec>>,
    panel_hits: Arc<AtomicUsize>,
    dismissals: Arc<AtomicUsize>,
}
impl Widget for Host {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: 10.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let spec = self.spec.lock().unwrap();
        let dismissals = Arc::clone(&self.dismissals);
        let scrim = spec.scrim.then(|| ScrimConfig {
            color: Color::rgba(0, 0, 0, 120),
            on_tap: Some(Arc::new(move || { dismissals.fetch_add(1, Ordering::SeqCst); })),
            exclude_rect: spec.exclude,
        });
        ctx.promote_at(
            spec.position.clone(),
            &Panel(Arc::clone(&self.panel_hits)),
            PromoteOpts { scrim, input: spec.input, focus: spec.focus },
        );
    }
}

struct App {
    spec: Arc<Mutex<Spec>>,
    panel_hits: Arc<AtomicUsize>,
    beneath_hits: Arc<AtomicUsize>,
    dismissals: Arc<AtomicUsize>,
}
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new()
            .child(Beneath(Arc::clone(&self.beneath_hits)))
            .child(Host {
                spec: Arc::clone(&self.spec),
                panel_hits: Arc::clone(&self.panel_hits),
                dismissals: Arc::clone(&self.dismissals),
            })
            .boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    panel_hits: Arc<AtomicUsize>,
    beneath_hits: Arc<AtomicUsize>,
    dismissals: Arc<AtomicUsize>,
}

fn harness(spec: Spec) -> H {
    let panel_hits = Arc::new(AtomicUsize::new(0));
    let beneath_hits = Arc::new(AtomicUsize::new(0));
    let dismissals = Arc::new(AtomicUsize::new(0));
    let e = FrameEngine::new(
        Box::new(App {
            spec: Arc::new(Mutex::new(spec)),
            panel_hits: Arc::clone(&panel_hits),
            beneath_hits: Arc::clone(&beneath_hits),
            dismissals: Arc::clone(&dismissals),
        }),
        FontCache::embedded(),
    );
    H {
        e,
        a: SkiaCanvas::new(WIN_W, WIN_H),
        b: SkiaCanvas::new(WIN_W, WIN_H),
        panel_hits,
        beneath_hits,
        dismissals,
    }
}

fn plain(position: LayerPosition) -> Spec {
    Spec { position, scrim: false, exclude: None,
           input: InputBehavior::PassThrough, focus: FocusBehavior::PassThrough }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn click(&mut self, x: f32, y: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
    /// The panel's own rect, found by type tag rather than by layer geometry —
    /// the layer spans the window when a scrim is present.
    fn panel_rect(&self) -> Rect {
        self.e.inspect_tree().iter()
            .find(|n| n.tag.ends_with("::Panel"))
            .and_then(|n| n.rect)
            .expect("the promoted panel painted")
    }
}

#[test]
fn centered_resolves_against_the_window() {
    let _guard = exclusive();
    let mut h = harness(plain(LayerPosition::Centered));
    h.frame();

    let r = h.panel_rect();
    assert_eq!(r.origin.x, (WIN_W as f32 - 100.0) / 2.0, "not horizontally centered");
    assert_eq!(r.origin.y, (WIN_H as f32 - 50.0) / 2.0, "not vertically centered");
}

#[test]
fn bottom_center_floats_above_the_bottom_edge() {
    let _guard = exclusive();
    let mut h = harness(plain(LayerPosition::BottomCenter));
    h.frame();

    let r = h.panel_rect();
    assert_eq!(r.origin.y, WIN_H as f32 - 50.0 - 16.0, "toast should float above the edge");
}

/// Without the clamp an anchored menu renders off-screen.
#[test]
fn an_absolute_position_past_the_edge_is_clamped_on_screen() {
    let _guard = exclusive();
    let mut h = harness(plain(LayerPosition::Absolute(Point { x: 5000.0, y: 5000.0 })));
    h.frame();

    let r = h.panel_rect();
    assert!(
        r.origin.x + r.size.width <= WIN_W as f32,
        "the panel runs off the right edge: {r:?}"
    );
    assert!(
        r.origin.y + r.size.height <= WIN_H as f32,
        "the panel runs off the bottom edge: {r:?}"
    );
}

/// A `Fill` overlay must be laid out TIGHT to the window. Loose constraints let
/// it collapse to its child, which put a bottom-right FAB's hit region at the
/// top-left over app content and stole clicks.
#[test]
fn fill_lays_out_tight_to_the_window() {
    let _guard = exclusive();
    struct Filler;
    impl Widget for Filler {
        fn layout(&self, c: &LayoutCtx) -> Size {
            Size { width: c.constraints.max_width_f32(), height: c.constraints.max_height_f32() }
        }
        fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(1, 2, 3)); }
    }
    struct FillHost;
    impl Widget for FillHost {
        fn layout(&self, c: &LayoutCtx) -> Size {
            Size { width: c.constraints.max_width_f32(), height: 10.0 }
        }
        fn paint(&self, ctx: &mut PaintCtx) {
            ctx.promote_at(LayerPosition::Fill, &Filler, PromoteOpts::default());
        }
    }
    struct FillApp;
    impl Component for FillApp {
        fn build(&self, _c: &mut Context) -> BoxedWidget { FillHost.boxed() }
    }

    let mut e = FrameEngine::new(Box::new(FillApp), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(WIN_W, WIN_H), SkiaCanvas::new(WIN_W, WIN_H));
    e.paint(&mut a, &mut b, &[]);

    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Filler")).and_then(|n| n.rect)
        .expect("the filler painted");
    assert_eq!(
        (r.size.width, r.size.height),
        (WIN_W as f32, WIN_H as f32),
        "a Fill overlay collapsed to {r:?} instead of filling the window"
    );
}

/// A tap outside the content dismisses; a tap on the content does not.
#[test]
fn a_scrim_dismisses_on_a_tap_outside_the_content() {
    let _guard = exclusive();
    let mut h = harness(Spec {
        position: LayerPosition::Centered,
        scrim: true,
        exclude: None,
        input: InputBehavior::Block,
        focus: FocusBehavior::Trap,
    });
    h.frame();
    let r = h.panel_rect();

    h.click(r.origin.x + 5.0, r.origin.y + 5.0);
    assert_eq!(h.panel_hits.load(Ordering::SeqCst), 1, "the content itself must stay clickable");
    assert_eq!(h.dismissals.load(Ordering::SeqCst), 0, "clicking the content must not dismiss");

    h.click(4.0, 4.0);
    assert_eq!(h.dismissals.load(Ordering::SeqCst), 1, "a tap on the scrim must dismiss");
}

/// A modal must not leak clicks to the page beneath it.
#[test]
fn a_blocking_overlay_absorbs_clicks_meant_for_the_page_under_it() {
    let _guard = exclusive();
    let mut h = harness(Spec {
        position: LayerPosition::Centered,
        scrim: false,
        exclude: None,
        input: InputBehavior::Block,
        focus: FocusBehavior::Trap,
    });
    h.frame();

    // Over `Beneath`, which occupies the top 200px and registers a hit.
    h.click(150.0, 30.0);
    assert_eq!(
        h.beneath_hits.load(Ordering::SeqCst),
        0,
        "a click passed through a blocking overlay to the widget underneath"
    );
}

/// The `exclude_rect` hole: a tap on the trigger that opened the overlay must
/// NOT be treated as a dismiss-tap, or the control can never close its own menu.
#[test]
fn the_scrim_exempts_the_rect_that_opened_it() {
    let _guard = exclusive();
    let trigger = Rect {
        origin: Point { x: 10.0, y: 10.0 },
        size: Size { width: 60.0, height: 20.0 },
    };
    let mut h = harness(Spec {
        position: LayerPosition::Centered,
        scrim: true,
        exclude: Some(trigger),
        input: InputBehavior::Block,
        focus: FocusBehavior::PassThrough,
    });
    h.frame();

    h.click(trigger.origin.x + 5.0, trigger.origin.y + 5.0);
    assert_eq!(
        h.dismissals.load(Ordering::SeqCst),
        0,
        "tapping the excluded trigger rect fired the scrim's dismiss — the dropdown would \
         close and immediately reopen on a single click"
    );

    // Everywhere else on the scrim still dismisses.
    h.click(200.0, 380.0);
    assert_eq!(h.dismissals.load(Ordering::SeqCst), 1, "the rest of the scrim must still dismiss");
}
