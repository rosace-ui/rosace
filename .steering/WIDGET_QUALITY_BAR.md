# Widget Definition of Done (the Quality Bar)

> A widget is **not done** until every *applicable* box below is checked.
> "Applicable" = the item makes sense for this widget (a `Divider` has no
> interaction states; a `Switch` has all of them). If an item doesn't apply,
> write one line in the PR/commit saying why — never silently skip.
>
> The reference implementation is [`rosace-widgets/src/tree/switch.rs`](../rosace-widgets/src/tree/switch.rs)
> (Q0 exemplar). When in doubt, match it.

Quality above quantity. We mature the library before we grow it.

---

## 1. States (visual)
- [ ] **Default** — looks premium with *zero* configuration
- [ ] **Hover** — a state layer / cursor change on pointer-over (desktop)
- [ ] **Pressed / active** — clear press feedback (scale, ripple, or fill shift)
- [ ] **Focus-visible** — a focus ring/halo when reached by keyboard (not on mouse click)
- [ ] **Disabled** — dimmed, inert, still absorbs the event (no fall-through)
- [ ] **Selected / checked / on** — where the widget has a value
- [ ] **Indeterminate / partial** — where it applies (tri-state checkbox, etc.)
- [ ] **Loading** — async widgets show progress, never a blank/frozen box
- [ ] **Error / empty** — async/data widgets render a real error and empty state
- [ ] **Read-only** — distinct from disabled, where it applies

## 2. Motion
- [ ] Every **state change is animated** (color, elevation, transform) — no instant snaps on hover/press/toggle
- [ ] Motion honors the **theme's global animation config** (respects reduce-motion / disabled)
- [ ] Entrance/exit animated where it matters (overlays, list items) — but **not** blanket auto-animation in virtualized/recycled containers (Known Issue #11)
- [ ] Uses `animate_to` / multi-channel animation, not per-widget timers

## 3. Theming
- [ ] Every color/size/radius/shadow comes from **theme tokens** — no hardcoded hex except deliberate semantic constants (documented, e.g. a switch's white knob)
- [ ] Correct in **light *and* dark**
- [ ] **Platform-adaptive** where the control differs (Material vs Cupertino)
- [ ] Respects **density** where the theme defines it

## 4. Feedback & affordance
- [ ] Focus ring, hover cursor, ripple/highlight as appropriate
- [ ] Haptic hook on the meaningful action (where the platform supports it)
- [ ] **Interactive-by-identity** — always owns its hit region, wired or not, so a tap never falls through to a pannable surface behind it

## 5. Accessibility
- [ ] Declares a **semantic role** + label
- [ ] Exposes its **value/state** to screen readers
- [ ] **Keyboard-operable** (focusable, and activatable via Space/Enter where it's a control)
- [ ] Participates in correct **focus order**

## 6. Layout & responsiveness
- [ ] Sensible intrinsic size; respects incoming `Constraints`
- [ ] **≥44px effective touch target** for interactive controls (hit area may exceed the visual)
- [ ] **RTL** correct (uses logical, not physical, sides)
- [ ] Handles **content overflow** (clip/ellipsis/scroll) — never spills or panics

## 7. Content / async (data & media widgets)
- [ ] Async content drives **Loading → Loaded → Error** through the reactive system (re-renders on completion, no polling)
- [ ] **Placeholder** while loading (skeleton/blur/solid — a *widget*, not just a color)
- [ ] Cancels/cleans up on unmount (no leaked requests)
- [ ] Graceful **fade-in / transition** on content arrival (opt-in, per D111)

## 8. API & ergonomics
- [ ] **Stunning default, fully overridable** — great with no args, every visual overridable via a builder
- [ ] Consistent builder naming with the rest of the library (D093/D094)
- [ ] Composable (accepts `impl Widget` children where it should)
- [ ] Doc comment explains what it is and shows the common case

## 9. Testing & verification
- [ ] Unit tests: layout invariants + key paint assertions (draws the states it claims)
- [ ] A visual snapshot (at least an `#[ignore]`d PNG generator) for the states
- [ ] **Verified by eye** — rendered and *looked at* in light+dark before "done" (the dark-thumb-looks-like-a-hole class of bug is invisible to tests)
- [ ] Ideally exercised live in a running app for interactive widgets

---

## Sign-off
A widget PR/commit says **"passes WIDGET_QUALITY_BAR"** and notes any items marked N/A with a reason. If it can't hit an item because the *framework* can't yet (e.g. multi-value animation, keyboard activation), that's a framework gap to file — not a reason to lower the bar for the widget.
