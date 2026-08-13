# Widget Audit Results

Produced by running `WIDGET_AUDIT_PIPELINE.md` over all 76 widgets.
Failures only — PASS and justified N/A verdicts are omitted for signal.
Live-testing findings live separately in `WIDGET_FINDINGS.md`.


## Fix status

Updated as work lands. The audit is the backlog; this is the burn-down.

| | Item | State |
|---|---|---|
| B1 | Password plaintext on the a11y bus | **FIXED** 803cc93 |
| B2 | `Image::fit(..)` did nothing | **FIXED** 803cc93 |
| B3 | `AbsorbPointer` blocked only clicks | **FIXED** 803cc93 |
| B4 | `MAX_TRANSFORM_DIM` never enforced | open |
| P1 | `len()*0.6` text estimates (5 widgets) | **FIXED** 292dd7b |
| — | Icons scaled but were centred unscaled | **FIXED** ea9c8a0 (D134) |
| P2 | Fixed heights that clip scaled text (~25) | open — badge done |
| P3 | Hardcoded colours (~20 widgets) | **FIXED** f2f9628 |
| P4 | Touch targets under 44px (~15) | in progress — switch/checkbox/radio done |
| P5 | Theme-forced fields with no builder | **FIXED** f2f9628 (folded into P3: avatar, badge, chip, progress_bar, nav_rail, list_view, scroll_view, skeleton all moved to `Option<Color>` + builder) |
| P6 | No `.padding(..)` (most of the library) | open |
| P7 | No tests (~20 widgets) | open |
| A | Missing/incorrect semantics (~12) | open |

Every fix above was confirmed to FAIL against the unfixed code before
being called done, rather than assumed to work because the new test passed.


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

## Batches 2 and 5 — REAL BUGS, not rubric items

These four are not "widget misses a builder". They are defects.

- **B1 — password plaintext leaks to the accessibility tree.**
  `text_input.rs` L213-214 passes `&self.value` to `.value(..)`
  unconditionally. The *visual* path correctly substitutes bullets at L241,
  and `self.obscure` is checked at L242 and L386 — but never at the semantics
  call. So VoiceOver/TalkBack/AccessKit announce the real password. Privacy
  defect; fix first.
- **B2 — `Image::fit(..)` does nothing.** Every constructor sets
  `ImageFit::Contain` (L18, L26, L31) and `.fit()` is a real builder (L39),
  but `paint` blits into `dest_rect = width x height` (L61, L96-102) and
  never calls `compute_fit` — that lives only on the legacy non-tree paint
  path (L200-206, L262). Every image in every ROSACE app is stretched and
  aspect ratio is silently ignored while the API claims otherwise.
- **B3 — `AbsorbPointer` is not a barrier.** `pointer_mode == 2` is handled
  ONLY in the click hit-test (`render_tree.rs` L433-439). The hover walk
  (L525), long-press walk (L552) and L747 each check `== 1` but never `== 2`,
  so hover, long-press and scroll pass straight through to the content
  behind — which is exactly the modal-barrier use its own doc claims (L25-27).
  `IgnorePointer` is correct on all four paths.
- **B4 — `MAX_TRANSFORM_DIM` is declared and never enforced.**
  `transform_layer.rs` L25 is its only occurrence in the repo; `child_size`
  L57 goes straight into `TransformLayerEntry` L83 unclamped, so the D082
  4096 px physical cap is not applied on this path.

## Batches 2 and 5 — pattern reinforcement

- **P1/X1** `icon.rs` is a NEW variant of the scaling bug and a nastier one:
  `layout()` returns the raw `size` square (L254-256) while `draw_text_at`
  multiplies px by `text_scale` (`tree/mod.rs` L872-873) — but the centring
  metrics come from `ctx.font.glyph(..)` and `ctx.font.ascender(..)`, and
  **neither of those scales** (`font.rs` L623-625, L838-843), unlike
  `measure_text`/`line_height` which do. So the glyph is drawn larger than
  its reported box AND mis-centred, with the error growing linearly in the
  OS setting. Same class as the Dynamic Type fix already landed in
  `button.rs` L93-112 — icons were simply not swept when it went in.
- **P2** more fixed boxes: `dropdown` 36 L48 (also fixed 200 width, label
  never measured, so a long option runs under the chevron with no ellipsis) ·
  `fab` flat `size` square L80-82 · `icon` L254-256 · `date_picker` whole
  widget fixed L397 and it ignores incoming `Constraints` entirely ·
  `interactive_viewer` `BTN = 32.0` L223 · `tooltip` `h = font_size * 1.7`
  L106 using the UNSCALED size while the glyphs inside scale · `text_input`
  36 L31/L57 · `grid` bento hands children a fixed lattice rect with no
  measurement at all (L169-207 takes no `LayoutCtx`).
- **P3** more hardcoded colour: `icon` default `rgb(180,184,210)` L238 —
  `Icon` never reads `ctx.theme` at all, so EVERY unstyled icon ignores the
  theme · `image` four literals L111/L119/L126/L130 · `dialog` shadow L262
  and barrier L144 while `theme.colors.shadow` exists · `dismissible` red
  L154 while `colors.error` exists · `text_input`/`text_area` whole palettes
  (documented decision at text_input L101-103, but still a FAIL) ·
  `tooltip` L28-29 reads only the theme EXT, never `theme.colors`, so a
  light-theme app with no `TooltipStyle` gets a dark bubble · `focus_api`
  ring L55 · `selection.rs` `flat()` L53-56 and `text_edit` `CursorStyle`
  L605 — a non-violet accent gets violet selection and a violet caret.
- **P7** more untested: `custom_paint` · `divider` · `dropdown` (the single
  test only checks layout size) · `fab` · `image` · `wrap` ·
  `repaint_boundary` · `transform_layer` · `pointer` · `focus_api`.
  `repaint_boundary`, `transform_layer`, `pointer` and `focus_api` have **no
  caller anywhere in the repo** outside re-exports — the built-but-never-
  wired pattern again.

## Batches 2 and 5 — per-widget

- **`text.rs`** — T2 FAIL matters more than most: `size` is a plain field
  defaulting to 18.0 (L58) and the named styles hardcode 16/14/22/20/40
  (L242-260), so `Text` — the single most-used widget — is the reason
  `dialog`, `data_table` and others fail T2 downstream. Fixing `Text` fixes
  a chain.
- **`date_picker`** — no day cell declares semantics at all (the cell loop
  L314-340 only paints), so all 42 dates are invisible; the calendar cannot
  be operated non-visually. Day cells also have no hover/press feedback,
  unlike the header and chevrons in the same file.
- **`data_table`** — sort state is communicated by appending a literal
  `"^"`/`"v"` to the header label (L137-142); that is what gets announced.
- **`dismissible`** — no semantics at all, and no non-gesture path to
  `on_dismissed`, so swipe-to-delete is unreachable without a pointer.
- **`fab`** — registers a hit ONLY when `on_press` is `Some` (L152-156),
  breaking interactive-by-identity that `button.rs` L180-189 follows
  explicitly. Icon-only FABs announce the literal `"action"` (L97).
- **`repaint_boundary`** — staleness is keyed on `rect` and the atoms only
  (L37-56), so a child whose CONTENT changes (different `Text`, theme swap,
  `text_scale` change) replays a stale Picture forever.
- **`divider`** — "unset colour" sentinel is `alpha == 0` (L21/L49), so a
  deliberately transparent divider silently becomes the theme outline.
  `Option<Color>` is the library's own pattern elsewhere.
- **`screen_transition_view`** — during a transition BOTH screens paint, so
  a screen reader sees two full screen subtrees at once; no
  `exclude_semantics` on the outgoing side.
- **`focus_api`** — `paint` passes `ctx` straight through (L48) instead of
  `ctx.child(rect)`, unlike every other wrapper, so the wrapped widget shares
  the wrapper's node. May alias per-node state (anim channels, text-edit
  state). Its doc example uses `TextInput::new("Email")`, a signature that no
  longer exists.


## The tap-target approach (P4)

Reserved in **layout**, not by inflating hit rects. `MIN_TAP_TARGET` (44,
the iOS HIG figure; Material says 48) is a floor on the space a control
occupies; it paints its smaller visual centred inside that via
`centered_visual`. This is `MaterialTapTargetSize.padded` in Flutter.

Growing hit regions instead is the obvious move and it is wrong: a row of
20px radios spaced 24px apart would end up with overlapping 44px hit rects,
and then registration order silently decides which one a tap lands on. The
layout approach makes neighbours space themselves correctly for free.

Cost: layouts containing these controls get slightly taller. That is the
intended change — the controls were genuinely too small to hit.
