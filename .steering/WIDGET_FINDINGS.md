# Widget Findings Log

Two sources, kept together because they answer the same question — what is
actually wrong with the widgets:

1. **Live testing** (below) — the user's own widget-by-widget pass on real
   devices. This was previously held only in assistant memory and had already
   been lost once to a context reset; it lives in the repo now so it cannot be.
2. **Source audit** — `WIDGET_AUDIT_PIPELINE.md` run over all 76 widgets;
   results land in `WIDGET_AUDIT_RESULTS.md`.

Neither replaces the other. The audit finds what is provably wrong in source
(hardcoded sizes, missing semantics); live testing finds what is wrong on a
device (taps not landing, glyphs missing) and cannot be read off the code.

---

## Part A — Live findings, assistant session 2026-08-08 → 12

Found by running the real apps, not by reading code. None are in the list
below; all are unfixed unless noted.

| # | Finding | Platform | Status |
|---|---|---|---|
| L1 | `ListTile` rows report `[0,0][0,0]` accessibility bounds — announced correctly but a screen reader cannot locate them spatially. Seen on all 48 rows of the Widgets list in a `uiautomator dump`. | Android | OPEN |
| L2 | Accessibility **actions are a no-op on every platform** — `AXPress` (macOS), `performAction` (Android) and iOS activation all return without doing anything. A screen reader can read the UI but not operate it. Deliberate for now: activation must route back into the engine's dispatch path. | all | OPEN |
| L3 | Accessibility **focus is always the tree root** — the VoiceOver/TalkBack cursor cannot follow keyboard focus. | all | OPEN |
| L4 | Desktop a11y tree **retains the outgoing screen's nodes** after navigation, so both screens are exposed at once. The engine test asserts the tree settles correctly in-harness and passes, so the fault is in the sync gating or AccessKit's retention of orphaned nodes. | desktop | OPEN |
| L5 | Bottom sheets are **content-width, not full-width** (measured at 100 px in a 400 px window). Both Material and iOS make them full-width. | all | OPEN |
| L6 | **No contrast support** — `MediaQuery` has `text_scale`, `bold_text` and `reduce_motion` but no `high_contrast`, so Increase Contrast / Reduce Transparency (iOS) and high-text-contrast (Android) do nothing. | all | OPEN |
| L7 | `Drawer` has **no shape or padding customization** — no `.radius()`, no `.padding()`; it paints a hard `fill_rect`. `Sheet` has both. | all | OPEN |
| L8 | **19 widgets use a fixed height** that ignores OS text scale, and `badge`/`checkbox`/`radio` still size text with `len() * size * 0.6`, which sees neither per-glyph advances nor `text_scale`. | all | OPEN |
| L9 | **No `TextOverflow` API** — only `max_lines`, which hard-truncates with no ellipsis. Height-driven truncation now emits a `warn!`. | all | OPEN |
| L10 | Only **14 of 80** widget files read theme typography; only **5 of 76** expose `.padding()`. | all | OPEN |
| L11 | `Tooltip` never appeared when wrapping an interactive child (the normal case) — hover marks one topmost node, so the child won and the tooltip never saw it. | all | **FIXED** (`hovered_within`) |
| L12 | Tapping **inside** a sheet dismissed it — `OverlayKind::Sheet` was `PassThrough` where `Dialog` was `Block`. | all | **FIXED** |
| L13 | OS text scale grew glyphs but not boxes — `line_height` did not apply `text_scale` while `measure_text` did, so rows overflowed their dividers. | all | **FIXED** |
| L14 | Overlays could not animate at all: painted into a throwaway tree rebuilt every frame, so retained state was destroyed. Also the likely cause of the reported hang, since the animation never settled and forced frames forever. | all | **FIXED** (retained keyed trees) |

---

## Part B — Live findings, user pass 2026-08-03

User tested the showcase app's widget list live (mostly Android, some macOS) and gave this
full bug list (verbatim, numbered as given, listed bottom-to-top of the widget list per their
own note). **None of these are fixed yet** — this memory exists specifically because the same
list was promised once before ([[project_showcase_app_and_framework_fixes]]) and lost when
context cleared before it was ever delivered. Do not lose it again — triage/fix from this file,
not from conversation memory.

1. Hero: no widget to test, just a box and explanation.
2. Scaffold: not really structured, and not theme-aware.
3. FAB size is big.
4. AppBar doesn't have a shadow, it has a stroke.
5. AppBar: needs a little more height; shadow/elevation instead of stroke (same issue as #4).
6. BottomNavBar: good, but no icon-button — buttons are all squared so it doesn't read as a
   bottom-nav button; also not responsive like every other widget.
7. NavRail: not clear, doesn't respond to theme, needs more explanatory widgets.
8. Toast has a stroke that isn't needed; needs a placement option + styling; duration isn't
   adjustable, that control is missing.
9. Snackbar: okay.
10. Drawer opens sideways; bottom-sheet-style drawer is missing from the example — all
    variants should be demoed.
11. (Correction to #10) — there IS a sheet, but buttons inside the sheet don't respond.
12. Dialog: modal dialog not responding, styled dialog not responding.
13. Menu: buttons not responding, needs fixing.
14. Tooltip: testing on Android so long-press isn't wired for tooltip — should wire long-press
    as the mobile trigger. Tested on Mac: tooltip not visible at all.
15. TimePicker: looks beautiful, but none of the controls respond — always stays at 9:30.
16. DatePicker: also not responding.
17. SearchBar: same text-input issue as Android (see #40).
18. Skeleton: always at a fixed ~90° angle, no adjustable inclination; the circle variant
    renders square (looks like a barcode scanner) instead of round; needs color/angle
    customization.
19. RatingBar: looks good, can't test because it doesn't respond to clicks (same app-wide
    click-response issue as others).
20. Stepper: only the first step responds, the rest don't (same issue).
21. Expander: shadow looks broken/funny (reads as just another gray box) — shadow is broken
    everywhere apparently; icon missing (Android glyph issue, same as dropdown #32); should be
    renamed "Accordion" (the universal/standard name for this widget).
22. Tab: good, but needs more customization; the scrollable-tabs variant is missing; divider
    colors, overall shape need work; 2nd/3rd tab don't respond (same app-wide issue).
23. Carousel: looks good, but scrolling backward (previous) flickers — confirmed: scrolling
    left-to-right (going to the previous item) causes a visible flicker/reappear. Open
    questions: does it use a PageController? ScrollController? Does it support both vertical
    and horizontal? Can animation be controlled/configured?
24. DataTable: looks good, but can't test — 2D pan/scroll not responding.
25. Unclear what the difference is between "Table" and "DataTable" (two separate widgets?
    needs clarification/docs).
26. Wrap: working correctly.
27. Grid: looks good; it has a builder API — ALL multi-child container widgets should get a
    builder API (PageView, Carousel, ListView, Grid, Table, etc.), not just Grid.
28. AspectRatio: good.
29. Image: currently shows random placeholders — need real placeholder/asset/network states:
    an explicit placeholder widget, a loading state, and a broken-image fallback, all
    demonstrated.
30. Container: excellent; shadows look good, but user has a doubt they'll follow up on with a
    screenshot (not yet provided as of this memory).
31. SegmentedControl: good.
32. Dropdown: good now; icons missing (Android glyph issue, same class as #21); needs more
    customization examples (background color, shape).
33. Badge: okay.
34. Avatar: okay, but how does it work with an image — can it take an image source? (question,
    not confirmed broken).
35. Divider: okay.
36. Chip: okay — can corner shape be customized/adjustable?
37. Card: good.
38. Progress: good.
39. Question: what's the actual difference between Progress and Slider (needs
    clarification/docs, not necessarily a bug).
40. TextInput: not working — Android issue (this is the same root issue #17/#23 etc.
    reference as "same app issue").
41. Button: needs a "Link" variant/type too (currently missing).
42. Switch: good.
43. Radio: good.
44. Checkbox: good.
45. The overlay FAB shows a Rust crab icon instead of the intended `</>` (code) icon —
    cosmetic/wrong-icon bug.
46. macOS: scroll has a momentum issue.

## Cross-cutting patterns worth root-causing first (not per-widget fixes)

Several numbered items above are surface symptoms of what looks like a **small number of
shared root causes** — worth investigating these before doing 46 individual widget fixes:

- **"Not responding to clicks/taps" is reported repeatedly on Android**: TimePicker (#15),
  DatePicker (#16), SearchBar/TextInput (#17, #40), Dialog (#12), Menu (#13), sheet buttons
  (#11), RatingBar (#19), Stepper steps 2+ (#20), Tab 2nd/3rd (#22), DataTable 2D pan (#24).
  This many independent widgets failing the same way on the same platform strongly suggests
  ONE shared Android input-dispatch or hit-testing bug, not 10 separate widget bugs. [[feedback_verify_dont_assume]]
  applies — investigate the shared input path on Android before touching individual widgets.
- **"Android glyph missing" reported on 2 widgets** (Expander #21, Dropdown #32) — likely the
  same icon-font-on-Android issue, one root cause.
- **"Shadow is a stroke instead of elevation/blur" reported on AppBar (#4/#5) and Expander
  (#21)** ("shadow broken everywhere") — likely one shared shadow-rendering bug, not
  per-widget.
- **Theme-awareness gaps reported on Scaffold (#2) and NavRail (#7)** — possibly the same
  class of bug already fixed once for ListTile/AppBar this session
  ([[project_showcase_app_and_framework_fixes]]) but not yet applied everywhere.

## Naming/API asks (not bugs)

- Rename Expander → Accordion (#21).
- Give every multi-child container widget (PageView, Carousel, ListView, Grid, Table) the
  same builder API Grid already has (#27).
- Give Button a "Link" variant (#41).
- Clarify Table vs DataTable distinction (#25), Progress vs Slider distinction (#39).

## L15 — engine text-input tests share state and flake (pre-existing)

Confirmed against an unmodified tree (stashed working copy): `cargo test -p
rosace --lib` fails roughly 1 run in 3, and **the failing test differs each
time** — `a_slow_second_click_does_not_count_as_a_double_click`,
`a_quick_press_and_release_does_not_trigger_long_press_select`,
`alt_arrow_moves_by_word_then_insert_lands_at_the_word_boundary` have all
been seen.

The symptom is not an off-by-one: the document itself comes back mangled.
One run expected `"hello woXrld"` and got `"ello worlXd"` — the leading `h`
is missing AND the insert landed at a different offset, which a caret-
position bug alone cannot produce. That points at keystrokes crossing
between concurrently running engines, not at the logic each test is
asserting.

Each of these builds its own `headless_text_input_engine()`, so the engine's
own click/caret state (`last_click_at`, `click_count`, …) is per-instance and
not the culprit. The shared thing is elsewhere — the focus registry, the
active-editable pointer, or the background long-press/caret timers (see
[[feedback_background_timer_test_races]], which records an earlier round of
exactly this class).

`ANIMATION_GLOBAL_TEST_LOCK` is held by some of these tests but plainly not
by all the ones that can interfere.

NOT caused by the widget audit work; logged here so it is not rediscovered.
Worth fixing before it erodes trust in the suite — a suite that fails 1 run
in 3 for unrelated reasons trains you to ignore red.

Fixed while confirming the above: `rosace-state`'s `dirty_set` tests raced
on the same process-global (each called `reset_to_global_dirty` at the top,
which is not enough when the suite runs in parallel). Now serialized.
