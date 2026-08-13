# Changelog

Notable changes per release. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Releases are tag-driven: pushing `vX.Y.Z` runs `.github/workflows/release.yml`,
which verifies the tag matches the workspace version, tests, dry-runs, then
publishes all 26 crates to crates.io in dependency order.

## [0.1.1] — UNRELEASED

> Prepared and verified, awaiting a `v0.1.1` tag. Everything below is on
> `main` but is **not on crates.io yet** — `cargo add rosace` still gives you
> 0.1.0.

First release since 0.1.0, and mostly a correctness pass: a systematic audit
of all 76 widgets against the project's own quality bar turned up defects that
had shipped unnoticed, including two that were silent data problems rather
than visual bugs.

### ⚠️ Breaking

Nominally a patch release, but two changes are source-breaking. Cargo treats
`0.1.0` and `0.1.1` as compatible, so `rosace = "0.1"` will pick this up
automatically — pin `=0.1.0` if you need the old surface.

- **`built_in::cupertino()` removed** (D133). ROSACE ships one design system.
  The Cupertino theme only restyled the accent colour and the app bar's
  alignment and height, which reads as iOS-ish without any of the structural
  work that makes a control feel native. The `Themes`/`Platform` bundle it
  used is unchanged and is exactly how a third-party skin plugs in.
- **39 crates consolidated into 26** (D131). Thirteen small crates were folded
  into the ones they were always used with. The public API is unchanged — the
  same items live at the same paths through `rosace` — but the individual
  crates are no longer published:
  `ws → net::ws`, `file → storage::file`, `forms → widgets::forms`,
  `scroll → widgets::scroll`, `clipboard → widgets::clipboard`,
  `shaping → text::shaping`, `bidi → text::bidi`, `a11y → core::a11y`,
  `i18n → core::i18n`, `nav-anim → nav::anim`, `ime → platform::ime`,
  `web-seo → platform::web_seo`, `gesture → platform::gesture`.

### Security & privacy

- **An obscured `TextInput` announced its plaintext to the accessibility
  tree.** The visual path substituted bullets and `obscure` was checked in two
  other places, but never at the `ctx.semantics(..)` call — so VoiceOver,
  TalkBack and AccessKit spoke the real password aloud. It now announces a
  bullet run: presence and length, never content.

### Added

- **Platform accessibility** (D132). One semantic tree now drives AccessKit on
  desktop, `UIAccessibilityContainer` on iOS and `AccessibilityNodeProvider`
  on Android, carrying stable ids and painted bounds.
- **`Semantics` widget** — annotate a subtree that cannot describe itself, or
  `exclude()` one that should not be announced at all.
- **`Responsive`** — build a different tree from the space available, the
  equivalent of Flutter's `LayoutBuilder`. Until now a layout could not react
  to its own size except through `RectReader`, which reports after paint and
  is therefore always a frame behind.
- `Role::SpinButton` and `Role::Menu`, mapped through all four platform
  bridges and the web SEO renderer.
- Builders that were missing: `ProgressBar::radius`/`label`, `Slider::label`,
  `Stepper::label`, `Dropdown::label`, `Dismissible::semantic_label`,
  `ListView::scrollbar_color`, `WithFocus::radius`/`ring_color`.
- `Role` is now exported from the crate root and prelude — `Semantics` was in
  the prelude but its `.role(..)` argument was not, so the widget could not be
  used from a plain prelude import.

### Fixed

- **Dropped keystrokes with more than one engine alive.** The dirty set was
  process-global while the state store it partners with was thread-local;
  `take_dirty_components()` drains, so one engine consumed another's marks and
  skipped the rebuild that would have applied the character. Now thread-local.
- **`Image::fit(..)` did nothing.** Every constructor set `Contain` and the
  builder existed, but the paint path never consulted it, so every image in
  every app was stretched and aspect ratio silently ignored.
- **`AbsorbPointer` was not a barrier.** Only the click walk checked for it;
  hover, long-press and click-to-edit passed through, and the scroll and zoom
  walks checked neither pointer mode.
- **Flex children were handed an infinite cross axis** inside a `ScrollView`,
  producing text positioned at NaN and a panic in the glyph walk.
- **Icons scaled with OS text size but were centred with unscaled metrics**
  (D134), so at 150% Dynamic Type a 24px icon drew at 36px, mis-centred.
  Icons now keep their designed size, as Flutter and Compose do.
- Text was sized by `len() * size * 0.6` in five widgets — an estimate that
  cannot tell `WWWWW` from `iiiii` and ignores OS text scale entirely.
- Sixteen widgets painted hardcoded colours instead of theme tokens. `Icon`
  never read `ctx.theme` at all, so every unstyled icon ignored the palette.
- Touch targets below the 44px minimum on switch, checkbox, radio, button,
  chip, dropdown, segmented and stepper. Reserved in layout, so the visual is
  unchanged and only the spacing grows.
- Widgets that announced nothing (`ListView`, `Menu`, `PullToRefresh`,
  `Dismissible`, `LongPressable`), announced the wrong role (`Stepper` claimed
  `Slider`, offering continuous-adjust gestures for a two-button control), or
  painted state they never exposed (`ListTile` selection, `NavRail` active
  destination, `Dropdown` expanded/collapsed).
- Tapping inside a `Sheet` or `Drawer` dismissed it; `non_dismissible()` added.
- Tooltips never appeared when wrapping an interactive child.
- `MAX_TRANSFORM_DIM` was declared and never enforced.

### Known gaps

- **Accessibility actions are still no-op on every platform.** Roles, labels
  and values are announced; activating a control from a screen reader is not
  wired up. `PullToRefresh` and `Dismissible` therefore announce affordances
  they cannot yet offer without a pointer.
- `DatePicker`'s day cells declare no semantics, so the calendar cannot be
  operated non-visually.
- Focus tracking always reports the root node.

## [0.1.0] — 2026-08-05

First public release. 39 crates published to crates.io.

[0.1.1]: https://github.com/rosace-ui/rosace/releases/tag/v0.1.1
[0.1.0]: https://github.com/rosace-ui/rosace/releases/tag/v0.1.0
