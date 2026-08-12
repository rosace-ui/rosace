# Widget Audit Results

Produced by running `WIDGET_AUDIT_PIPELINE.md` over all 76 widgets.
Failures only — PASS and justified N/A verdicts are omitted for signal.
Live-testing findings live separately in `WIDGET_FINDINGS.md`.

## Systemic patterns (fix these once, not per-widget)

**P1 — `len() * size * 0.6` text estimate (X1).** Sizes text by BYTE length
with a flat advance and, critically, never sees `MediaQuery::text_scale`
(only `measure_text` applies it). The widget's own `paint` then measures for
real, so reported width and drawn text disagree and the label overflows at
raised Dynamic Type. `button.rs` L93-100 documents this exact bug being
fixed live on iOS; the rest of the codebase was never swept.
> `badge` L57 · `checkbox` L81 · `bottom_nav` L218 (magic 7.0)

**P2 — fixed height ignoring text scale (X2).** A designed height used as a
CEILING rather than a minimum, so scaled-up text clips. Fix pattern is
`self.height.max(line_height + pad)` — `button.rs` L112, `app_bar.rs` L89.
> `accordion` HEADER_H 44 L52 · `autocomplete` 36 L45 · `avatar` L31 ·
> `badge` 16 L58 · `bottom_nav` 56 L108 · `chip` 30 L62 ·
> `search_bar` 36 L31 · `segmented` 34 L23 · `snackbar` 46 L46 ·
> `stepper` 32 L55 · `tab` 40 L59 · `tabs` 40 L79 (compounds `tab`'s)
> Note the pattern: several grow HORIZONTALLY with measured text and miss
> only the vertical axis — the omission is consistent, not random.

**P3 — hardcoded colours (T1).** Identical in light and dark; several are
actively wrong on one theme.
> `avatar` L19-20 · `badge` L32-33/L39-40 (`theme.colors.error` exists and
> `bottom_nav` L122 already uses it for the same badge) ·
> `button` Danger/Success L126-127 · `card` shadow pure-black L72 ·
> `circular_progress` track white-on-white, invisible in light theme L51 ·
> `skeleton` shimmer hardcoded white L24/L33, near-invisible on a light
> theme · `toast` pure-black shadow L115 while `theme.colors.shadow` exists
> (`toast`'s Success green L84 is defensible — ColorScheme has no success
> token)

**P4 — touch targets under the 44 px Quality Bar §6 minimum (I2).**
Hit region equals `ctx.rect` in each case, with no expanded touch area.
> `slider` 24 px (worst) · `switch` 44x24 · `checkbox` ~21 px · `chip` 30 px ·
> `stepper` 32x32 · `button` ~34 px · `segmented` 34 · `autocomplete` 36 ·
> `search_bar` 36 · `tab`/`tabs` 40 (and `tab` also drops under 44 px WIDE
> at ~6 tabs on a phone, since width is `bar_width / n`)
> `switch` is the Quality Bar's own exemplar widget, so this particular
> gap propagates by imitation.

**P5 — theme-defaulted but not overridable (S3).** `Option<f32>` field with
no builder, so theme-defaulted is effectively theme-FORCED — the opposite
of the "max-customizable" standard.
> `badge` font_size no builder L15 · `chip` font_size AND height, both
> private so not even a struct-literal escape hatch L25-26 · `avatar` L11

**P6 — no `.padding()` (S2).** Insets baked into paint arithmetic.
> `accordion` PAD_H L53 · `app_bar` 5 separate literals · `badge` +12 L57 ·
> `bottom_nav` · `button` +32/+16 L108/L112 · `checkbox` gap 10 L81/L185 ·
> `chip` +28 L61

**P7 — no tests (V1).**
> `accordion` · `aspect_ratio` · `avatar` · `badge` · `circular_progress` ·
> `spacer` · `stack` (both fit modes and paint order entirely unverified)

## Per-widget notes worth acting on separately

- **`checkbox`** — `indeterminate_draws_a_dash_not_a_tick` (L244) asserts the
  absence of `U+2713`, but the widget now draws `U+E668` (L160). The test
  passes even if a tick IS drawn: it no longer tests its own claim.
- **`bottom_nav`** — selected state never reaches `SemanticsProps` (A2). A
  screen reader cannot tell which destination is current. Also hover/press
  SNAP rather than ease (raw booleans, not `animate_to`) — Quality Bar §2.
- **`autocomplete`** — expanded/collapsed state never announced (A2); a
  screen reader hears a plain text field, not a combobox. Keyboard
  operability UNKNOWN — no arrow/Enter handling in the file.
- **`avatar`/`badge`/`circular_progress`** — announce raw content with no
  `.semantic_label()` escape hatch, so three progress rings on one screen
  are indistinguishable ("Progress", "Progress", "Progress").
- **`accordion`** — the comment at L148 says the body fades along `t`; the
  code paints it unconditionally at full opacity. Comment and code disagree.
- **`card`/`container`** — shader `.material()` paints square corners under a
  rounded box (known, documented, D124 Step 4+).
- **`container`** — `EdgeInsets` and `align.offset` are physical, not
  logical, so it cannot mirror under RTL. Framework-level, not a file defect.
- **`app`** — `WidgetApp::new` `expect`s a system font and panics rather than
  falling back to `FontCache::embedded()`, which every widget test uses.

## Batch 4 additions — per-widget

- **`slider`** — declares `Role::Slider` with a value but **no label**, and
  there is no `.label(..)` builder (contrast `switch` L67). A screen reader
  announces a bare number with no idea what it controls. (A1 FAIL)
- **`stepper`** — declares `Role::Slider`, which is the **wrong role**: this
  is a discrete two-button numeric control, so assistive tech offers slider
  gestures for something that only supports two presses. Its label is also a
  hardcoded, non-localizable "stepper". (A1 FAIL)
- **`snackbar`** — the action button has **no hover, press or disabled
  feedback at all**; it is plain text with a hit region. (I3 FAIL)
- **`switch`** — takes a `FocusNode` via `ctx.focus_node()` but never calls
  `ctx.register_focus(..)` (contrast `text_input` L217). Whether it is
  Tab-reachable or Space/Enter-activatable is UNKNOWN and needs a driven
  key-event test — a Quality Bar §5 keyboard question on the exemplar widget.
- **`search_bar`** — its inner `TextInput` is built once into a `OnceLock`
  from the values present at FIRST paint. Whether a later `.value(..)` from
  the app reaches the field is not decidable from source. Real risk, since
  a search field whose value cannot change would be badly broken. Settle by
  painting twice with different values.
- **`sheet`** — correctly declares `Role::Dialog` as a modal boundary, but
  has no `.title(..)` API, so assistive tech entering it hears only "dialog".
- **`skeleton`** — reads `anim_clock()` raw and calls `request_animation()`
  unconditionally rather than going through `animate_to`, so whether it
  honours reduce-motion (Quality Bar §2) is UNKNOWN.
- **`table`** — layout is O(rows²·cols): `row_range` re-sums the prefix for
  every access and `cell()` is called inside both loops. Also the zebra
  stripe is hardcoded to `row % 2 == 1` with no offset, so a table with a
  header row stripes the wrong rows.
- **`tab`** — no focus-visible ring, unlike `segmented` L115-118.

## Batch 3 additions — pattern reinforcement

- **P1** (`len()*0.6`) two more: `radio` L57, `nav_rail` badge L112 (`* 7.0`).
- **P2** (fixed height) `menu` row 34 L41 · `nav_rail` item 36 L33, section 20
  L191, badge 16 L114 · `list_tile` title-only branch pinned to 48 L119 (the
  subtitle branch DOES grow — L117, so the fix is one line) · `scaffold`
  measures the app bar with `Constraints::tight(w, 44.0)` L76 and the bottom
  bar `tight(.., 48.0)` L81, so a bar that grew for large type cannot even
  report the height it needs.
- **P3** (hardcoded colour) `list_tile` press/hover wash is white L151 —
  invisible on a light theme, and every OTHER colour in that file resolves
  correctly (L155-159) · `list_view` scrollbar L54 · `scroll_view` scrollbar
  default L99 · `progress_bar` BOTH colours L19-20 (no `ctx.tc` in the file
  at all) · `nav_rail` chrome L142-143, L198, L206, L117.
- **P4** (<44 px) `menu` 34 · `nav_rail` 36 · `radio` ~20x24 · `rating_bar`
  20x20 per star · `pressable` has NO floor — `Text::new("x").on_press(..)`
  is a one-glyph target.
- **P6** (no `.padding()`) `list_tile` (bare pub field, no setter) ·
  `list_view` · `menu` · `nav_rail` · `scroll_view`.
- **P7** (no tests) `list_view` · `nav_rail` · `padding` · `positioned` ·
  `row` · `column` · `scaffold`. `row`/`column` are the worst of these: the
  flex pool, the unbounded-axis guard, and the "paint must never re-measure"
  rule all carry comments describing real bug classes and none is pinned.

## Batch 3 — per-widget

- **`list_view`** — no `ctx.semantics` at all: no `Role::List`, no item
  count, and rows outside the viewport are never built (L95-109), so
  assistive tech cannot discover them either. Also L122 `bar_y` is unclamped,
  so at max scroll the thumb runs past the track bottom by up to `bar_h`;
  `scroll_view` L671-673 clamps exactly this.
- **`pull_to_refresh`** — no semantics at all (L78-133): the affordance is
  invisible, `refreshing` is never announced, and there is no non-gestural
  way to trigger it. Its two tests check `layout` and field assignment; the
  actual trigger behaviour is untested.
- **`pressable`** — `Pressable` itself is exemplary (deliberately absent
  label so platform a11y derives the name from descendants, rationale
  L37-42), but **`LongPressable` declares no semantics at all** (L62-70), and
  the blanket `PressApi` (L74-83) puts it on every widget in the library.
- **`progress_bar`** — `Role::ProgressBar` with a value and no label and no
  `.label(..)` builder. Also no indeterminate mode, so every "loading,
  extent unknown" case must invent a fake value.
- **`scaffold`** — `content_h = total.height - bar_h - bottom_h` L94 is not
  clamped to zero, and the safe-area rect ten lines up (L67-71) uses
  `.max(0.0)` for exactly this class of problem. A window shorter than the
  bars yields negative child rects.
- **`scroll_view`** — the strongest file audited: `resolve_physics` (L152-156)
  is the reference implementation of S3, and the physics comments record real
  trackpad findings instead of asserting correctness. Its one real gap is
  that the scrollbar thumb is decorative — no hit region anywhere in
  L629-742, so a pointer user cannot grab it.
- **`menu`** — no `Role::Menu` container around the items, so screen readers
  get loose menu items with no group or count.
- **`nav_rail`** — `active` is never announced (L63-65), so a screen reader
  cannot tell which destination is current.
- **`rating_bar`** — a11y label is the hardcoded English `"rating"` L107.
- **`radio`** — `.size(s)` silently overwrites `font_size` with `s * 0.65`
  (L35), so asking for a bigger control opts the label out of theme
  typography without saying so.
- **`padding`** — there is **no `Padding` widget** in the library; padding is
  a field on `Row`/`Column` only, so a single arbitrary child cannot be inset
  without wrapping it in a `Column`. `EdgeInsets` is also physical
  left/right, so RTL is not expressible through it — and every consumer
  inherits that (`row`, `column`, `positioned` all repeat it).
