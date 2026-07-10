# Phase 4 — Rich Widgets, Forms & Multi-Screen Demo

> Status: IN PROGRESS
> Started: 2026-06-27
> Target: Full widget library, form system, image rendering, and real multi-screen demo app

## Steps

### Step 1 — Rich widget library
New widgets added to `rosace-widgets`:
- `Checkbox` — checked/unchecked/indeterminate with animated indicator
- `Switch` — boolean toggle with slide animation
- `Slider` — continuous value picker, min/max/step, labeled
- `ProgressBar` — determinate (0.0–1.0) and indeterminate pulse
- `Badge` — numeric/dot overlay for notification counts
- `Chip` — selectable label pill, dismissible variant
- `Avatar` — circular image/initials placeholder
- `Tooltip` — floating label on hover (render as adjacent text for now)
- All themed variants using `ThemeData` color tokens

### Step 2 — Image rendering (D033)
- `ImageWidget` — loads PNG bytes via `tiny_skia::Pixmap::decode_png()` and blits to canvas
- Supported sources: file path, `&[u8]` bytes, URL stub (returns placeholder)
- `ImageFit` enum: Fill, Contain, Cover, None
- Lives in `rosace-widgets/src/image.rs`

### Step 3 — Modal / Dialog overlay
- `Modal` — full-screen dim overlay with centered content box
- `Dialog` — title + message + buttons (OK/Cancel pattern)
- `Toast` — transient bottom notification with auto-dismiss timer
- Lives in `rosace-widgets/src/overlay.rs`

### Step 4 — `rosace-forms` crate
- `FormField<T>` — wraps an Atom<T> with validation state
- `Validator` trait + built-in validators: `Required`, `MinLength`, `MaxLength`, `Pattern`, `Range`
- `Form` — collects multiple FormFields, exposes `validate_all() -> bool`, `errors() -> Vec<FieldError>`
- `FieldError` — field name + message
- Submission handled by the app (Form doesn't own the submit action)

### Step 5 — Multi-screen navigation demo
- New example: `rosace-examples/src/bin/nav_demo.rs`
- 3 screens: Home → Profile (with form) → Settings
- Uses `Navigator<Screen>` push/pop, back button (Backspace key)
- Profile screen has a name text field with required validation
- Settings screen shows a theme toggle (light/dark)
- Renders in a 640×480 live window (RosaceApp)

### Step 6 — Phase 4 showcase (static PNG)
- New example: `rosace-examples/src/bin/phase4_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Widget Gallery — Checkbox, Switch, Slider, ProgressBar, Badge, Chip
  2. Image Panel — ImageWidget with tiny-skia PNG blit
  3. Forms Panel — FormField with validation errors rendered
  4. Navigation Panel — Navigator stack diagram (boxes + arrows)
- Proves all Phase 4 systems work together

## Exit Criteria

- [ ] All 8 new widgets render correctly and have themed variants
- [ ] `ImageWidget` blits a PNG file without panicking
- [ ] `Modal` dims content behind it; `Toast` auto-dismisses after 3 seconds of frames
- [ ] `rosace-forms` validates all built-in rules and reports errors
- [ ] Nav demo runs as a window with 3 screens and working back navigation
- [ ] Phase 4 demo PNG is saved to `phase4_demo.png`
- [ ] Full workspace tests pass with zero warnings
- [ ] `cargo build --release` is clean

## Approved dependencies

- `tiny_skia` already in `rosace-render` — use `Pixmap::decode_png()` for image loading
- No new external crates without discussion
- `regex` may be added for `Pattern` validator if needed (already common in Rust ecosystem)

## DO NOT

- DO NOT add GPU rendering — stays tiny-skia until v1.0
- DO NOT implement animation-driven transitions between screens — Phase 5
- DO NOT add async form submission — forms are synchronous validate-and-read
- DO NOT add drag-and-drop — Phase 5
- DO NOT implement URL routing for web — stub only (Phase 3 deferred)
