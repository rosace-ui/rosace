# Animation

ROSACE has two layers of animation. A small, curated set of widgets animate **automatically** — you write no animation code at all. Everything else — animating a value your own component owns — is **explicit**: you call a hook, and it drives a value forward one frame at a time until you stop asking for it.

## What's automatic (and what isn't)

Governed entirely by the theme's `AnimationConfig` (see [Theming](theming.md)), the following animate with zero app code: press/tap feedback, momentum scroll, default navigation transitions, and the toggle-state widgets — `Switch`, `Checkbox`, `Radio`, `SegmentedControl`. Turn all of it off at once with `rosace::theme::set_animations(false)`.

That's the whole list. An earlier design tried to make animation universal — including per-item content inside virtualized containers like `ListView` — and it was reverted: `ListView` recycles render-tree slots positionally as you scroll, so a "fade in" tied to a slot could land on the wrong row's data or resume a stale animation mid-flight. If you want an image, a list row, or any other custom value to animate, you drive it yourself with one of the hooks below — there is no default animation on `Image` or on content inside a recycled list.

## `use_animation`: time-based progress

`use_animation` is a component hook — call it inside `build`, in a stable position, like `ctx.state`. It gives you a `Progress` (0.0 → 1.0 over a fixed duration) and an `AnimCtrl` to drive it:

```rust
use std::time::Duration;
use rosace::prelude::*;
use rosace_animate::use_animation;

impl Component for GrowingBox {
    fn build(&self, ctx: &mut Context) -> Element {
        let (progress, ctrl) = use_animation(ctx, Duration::from_millis(400));

        Column::new()
            .child(Button::new("Play").on_press({
                let ctrl = ctrl.clone();
                move || ctrl.play()
            }))
            .child(
                Container::new()
                    .width(20.0 + progress.get() * 180.0)
                    .height(40.0)
                    .background(Color::rgb(102, 102, 230)),
            )
            .into_element()
    }
}
```

- `ctrl.play()` starts (or resumes) the animation; `ctrl.pause()` freezes it; `ctrl.reset()` rewinds to 0 and goes idle.
- `ctrl.set_repeat(true)` loops it; `ctrl.set_reverse(true)` outputs `1.0 → 0.0` instead of `0.0 → 1.0`.
- Once running, `use_animation` advances the underlying `AnimationController` by the real frame delta every frame and writes the result to a persistent atom — that atom write is what schedules the next frame, so you never call a `tick()` yourself.
- `progress.get()` is **linear** — `use_animation` doesn't apply an easing curve for you. Shape the motion by running it through an `Easing` yourself:

```rust
use rosace_animate::Easing;

let eased = Easing::EaseOutBack.eval(progress.get());
```

Available `Easing` variants: `Linear`, `EaseIn`/`EaseOut`/`EaseInOut`, the quad variants, `EaseInBack`/`EaseOutBack` (slight overshoot), `EaseOutBounce`, and `CubicBezier(x1, y1, x2, y2)` for a CSS-style custom curve.

## `use_spring`: physics-based motion

For motion that should feel alive rather than run a fixed duration — draggable panels, snapping toggles, anything reacting to a moving target — use a spring instead of a tween:

```rust
use rosace::prelude::*;
use rosace_animate::use_spring;

impl Component for ExpandingPanel {
    fn build(&self, ctx: &mut Context) -> Element {
        let expanded = ctx.state(false);
        let (height, ctrl) = use_spring(ctx, 60.0);

        // Re-target the spring whenever the state that drives it changes.
        ctrl.animate_to(if expanded.get() { 240.0 } else { 60.0 });

        Column::new()
            .child(Button::new("Toggle").on_press({
                let expanded = expanded.clone();
                move || expanded.update(|e| !e)
            }))
            .child(Container::new().height(height.get()))
            .into_element()
    }
}
```

- `use_spring(ctx, initial)` returns an `Animated` (read `height.get()` each frame) and a `SpringController`.
- `ctrl.animate_to(target)` re-aims the spring — call it every `build` with whatever the current target should be; it's a no-op in terms of motion if the target hasn't changed.
- `ctrl.snap_to(value)` jumps instantly with zero velocity — useful for resetting without a visible animation.
- Tune feel with `.stiffness(k)` (higher = snappier, default 200) and `.damping(d)` (higher = less oscillation, default 20), chained off the controller.
- Like `use_animation`, the spring advances on its own every frame it hasn't settled (position and velocity both near the target) — no manual driving needed. It sub-steps internally to stay numerically stable even at low frame rates.

## `Keyframe`: multi-stop sequences

For a value that should pass through more than two points — not just A→B — build a `Keyframe<T>` and evaluate it against a progress value from `use_animation`:

```rust
use rosace_animate::{Keyframe, Easing};

let seq = Keyframe::new()
    .stop(0.0, 0.0_f32)
    .stop_with_easing(0.5, 1.3, Easing::EaseOut)   // overshoot
    .stop(1.0, 1.0);

let scale = seq.eval(progress.get()).unwrap_or(1.0);
```

Each stop is a normalized time `t ∈ [0, 1]`, a value, and the easing applied to the segment leading *into* the next stop. `Keyframe<T>` works for any type implementing `Lerp` — built in for `f32`, `f64`, `[f32; 2]`, and `[f32; 4]` (handy for interpolating a color or a point as a raw array).

## Lower-level primitives: `Tween` and `Spring`

`use_animation`/`use_spring` are hooks — they own the persistent state for you via `ctx`. Underneath, `rosace-animate` also exposes the raw building blocks they're built on: `Tween<T>` (wall-clock-driven, one `.value()` call returns `(current, is_complete)`) and `Spring` (the same physics `use_spring` wraps, minus the hook plumbing). Reach for these only if you're managing animation state outside the normal `ctx.state` component lifecycle — e.g. inside a custom widget's own persistent render-tree node. For component code, the hooks above are the API you want.

---

**Under the hood:** how a hook's persistent atom write schedules exactly one more frame (and why that's safe to call every `build`) is the same mechanism covered for `ctx.state` in [Core: Component, Element, Context](../architecture/core.md).

Next: [Persistence & Networking](persistence-networking.md).
