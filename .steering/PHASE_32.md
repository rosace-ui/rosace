# Phase 32 — Widget Expansion, Extensible Icons, Rich Text/Emoji (D115)

> Status: Scoped, not started.
> Started: —
> Completed: —
> Decision: **D115** — add the widgets a typical real app needs and
> doesn't have today; replace `Icon`'s closed hardcoded-enum system
> with something third parties can extend; wire the already-built
> `rosace-text` `RichText`/`TextSpan` into the `Text` widget; add
> color-glyph (emoji) support as part of Phase 27 Step 4's glyph atlas
> work. Sequenced after Phase 27 so new widgets are built against the
> GPU-native `DrawCommand` set once, not built twice.

## Why This Phase

User: "we need more widgets honestly." Checked the actual widget set — 55 files in `rosace-widgets/src/tree/`. Core controls/layout/overlays are solid, but grepping for common real-app widgets found zero hits for: `FloatingActionButton`, `BottomNavigationBar` (there's `NavRail`, a side rail, but no bottom bar), `DatePicker`/`TimePicker`, `Carousel`/`PageView`, `Stepper`, `SearchBar`, `RatingBar`, `DataTable`, `Snackbar`.

Separately: `Icon` (`icon.rs`) is a closed `enum IconKind` of 27 hand-drawn vector shapes — a third party cannot add an icon without editing core `rosace-widgets` code, which conflicts with the whole point of `WIDGET_AUTHORING_GUIDE.md`'s escape-hatch philosophy.

And: `rosace-text`'s `RichText`/`TextSpan` (multi-style spans within one paragraph) exist and compile but are completely unused — the real `Text` widget only calls `rosace_text::word_wrap` for single-style wrapping. Same "built, never wired" pattern as `rosace-forms` (D112) and `ScrollView`/`Navigator`/`ImageCache` before it.

And: zero emoji/color-glyph support exists anywhere (no COLR/bitmap-glyph handling found). This needs to be designed into Phase 27 Step 4's glyph atlas from the start — atlas entries need a "this glyph is a color bitmap, not a monochrome mask" distinction baked into the format, which is much cheaper to add while building the atlas than to retrofit after.

## Out of Scope (deliberately, not silently dropped)

- **Charting/graph widgets.** Real, but a large surface of its own (data-to-visual mapping, axes, legends) — not "a widget," a sub-framework. Separate future phase if a real need surfaces.
- **A generalized icon-font-import pipeline** (arbitrary `.ttf`/`.woff` icon fonts a user drops in). Step 2 below picks ONE extensible mechanism and ships it well; a full import pipeline for arbitrary third-party icon fonts is follow-up work once the mechanism itself is proven.
- **`DataTable` sorting/filtering/pagination as built-in policy.** Ship the rendering primitive first (Step 1); interactive data-grid behavior (sort-on-click, column resize, virtualized rows reusing `ListView`'s pattern — CAREFULLY, given Known Issue #11) is real follow-up scope.
- **Full CJK/complex-script shaping.** Already deferred to v1.0 (D014, `FallbackShaper`'s one-glyph-per-char limitation) — unrelated to this phase's emoji/rich-text scope, not solved incidentally by it.

## Steps

### Step 1 — New widgets: the missing list
`FloatingActionButton`, `BottomNavigationBar`, `Stepper`, `SearchBar`, `RatingBar`, `Snackbar`, `DatePicker`/`TimePicker`, `Carousel`/`PageView`, `DataTable` (rendering only, per Out of Scope). Each follows `WIDGET_AUTHORING_GUIDE.md`'s taxonomy (Leaf/single-child/multi-child) and is built against Phase 27's GPU-native `DrawCommand` set — new shape needs (e.g. `DatePicker`'s calendar grid, `RatingBar`'s star shapes) should go through Phase 27's shader-based built-ins where they fit the existing vocabulary, or extend it, rather than reaching for CPU `tiny-skia` calls that Phase 27 is actively removing.

Exit: each widget compiles, has unit tests for layout, and is exercised in a real running app (extend `demo_app`, the kept showcase from Phase 26) — screenshotted, not just compiled.

### Step 2 — Extensible icon system
Replace `IconKind`'s closed enum with something a third-party crate can add to — most likely an icon font (glyphs in a font file, rendered through the same text/glyph-atlas pipeline Phase 27 Step 4 builds, so icons get GPU-native rendering for free) or SVG-path data registered by string key. Pick based on which fits the Phase 27 glyph atlas better — decide at Step 2's start, not before, since Phase 27's actual atlas shape isn't built yet as this doc is written.

Exit: an icon NOT in today's 27-shape list is added by a downstream crate (a real, separate test crate depending on `rosace-widgets`) without editing `rosace-widgets` itself, and renders correctly in a real running app.

### Step 3 — Wire `RichText`/`TextSpan` into `Text`
`Text` gains a way to render mixed-style spans (bold/italic/color changes mid-paragraph) using `rosace-text`'s existing `RichText`/`TextSpan`/`TextLayout` types — real integration, not a rewrite of those types.

Exit: a real running app renders a paragraph with at least two different inline styles (e.g. bold + a colored link-style span) in a single `Text` widget, verified live.

### Step 4 — Emoji / color-glyph support
Depends on Phase 27 Step 4's glyph atlas existing. Add color-bitmap glyph handling (most fonts ship emoji as embedded bitmaps or COLR/CPAL layered outlines) as a distinct atlas-entry kind alongside the atlas's default monochrome-vector glyphs.

Exit: a real running app renders a string containing at least one emoji correctly (real color, not a missing-glyph box), inline with regular text, verified live.

## Sequencing

Step 1 (new widgets) and Step 2 (icons) are independent of each other and of Steps 3-4, but both benefit from Phase 27 being done first (per the "build against the best render object once" reasoning). Step 3 and Step 4 are independent of each other but Step 4 strictly requires Phase 27 Step 4 (glyph atlas) to exist first — cannot start before that regardless of this phase's own internal ordering.

## Migration Rule

All additive — no existing widget's API changes. `Icon`'s existing `IconKind` variants keep working through Step 2's transition (old call sites don't break); if the mechanism genuinely can't preserve the old enum call sites, that's a real compatibility decision to surface explicitly at Step 2, not silently break.
