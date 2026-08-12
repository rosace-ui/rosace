# Widget Audit Pipeline

A mechanically checkable form of `WIDGET_QUALITY_BAR.md`. The Quality Bar is
the standard; this is how you *verify* a widget against it from source, so
the answer is evidence rather than opinion.

Why it exists: §5 (accessibility) had been in the Quality Bar since the
beginning and was never enforced, so widgets shipped silent. The same turned
out to be true of theme typography (widgets hardcoded 11 px titles while the
theme said 17) and of text scaling (fixed heights that ignore the OS setting).
Each was found by accident, one widget at a time. This pipeline finds them all
at once.

## Rules for an auditor

1. **Read the source. Cite line numbers.** "Looks fine" is not a result.
2. **Never guess a verdict.** If a check needs runtime behaviour you cannot
   see from source, mark `UNKNOWN` and say what would settle it.
3. **`N/A` is a real verdict and needs a reason.** A `Spacer` has no text, so
   T2 is N/A — that is not a failure. A layout primitive being silent to a
   screen reader is correct, not a gap.
4. **Do not change any code.** Audit only. Fixes come after triage.

## Checks

### T — Theme
- **T1 colours from tokens** — every colour resolves from `ctx.theme.colors.*`
  (or an explicit builder override). FAIL on hardcoded `Color::rgb(...)`
  literals in paint, except documented semantic constants.
- **T2 type from tokens** — font sizes resolve from `theme.typography.*`.
  FAIL on hardcoded sizes like `title_size: 11.0`.
  The good pattern: `Option<f32>` field + `resolved_*(theme)`, as in
  `badge.rs` / `button.rs` / `list_tile.rs`.

### S — Shape & customization
- **S1 shape** — exposes `.radius(..)` (or documents why it has no shape).
  `Sheet` has one; `Drawer` does not and paints a hard `fill_rect`.
- **S2 spacing** — exposes `.padding(..)` where it wraps content. Only 5 of
  76 widgets do today.
- **S3 overridable defaults** — theme-defaulted must not mean theme-forced:
  an explicit builder value always wins.

### A — Accessibility (Quality Bar §5)
- **A1 role + label** — declares `ctx.semantics(SemanticsProps::new(role))`
  with a label, OR is correctly transparent (layout primitives, decorative
  content). Enforced by
  `widgets_meet_quality_bar_section_5_semantics` in `rosace/src/engine.rs`.
- **A2 value/state** — controls expose their state via `.value(..)` — a
  checkbox's checked-ness, a slider's number. A label alone is not enough.
- **A3 actionable roles** — a widget with a press handler declares an
  interactive role, not a structural one. `ListTile` announced `ListItem`
  while being tappable, so screen readers offered no action.

### X — Text scaling (OS Dynamic Type)
- **X1 measures text** — uses `font.measure_text(..)` / `font.line_height(..)`
  (both apply `MediaQuery::text_scale`). FAIL on estimates like
  `label.len() as f32 * size * 0.6`, which ignore both per-glyph advances and
  the OS setting.
- **X2 grows with content** — no hardcoded height that clips scaled text.
  A designed height is fine as a MINIMUM (`self.height.max(line_h + pad)`).

### I — Interaction
- **I1 hit region** — interactive widgets declare `ctx.on_press`/`hits`.
- **I2 touch target** — ≥44 px effective (Quality Bar §6); the hit area may
  exceed the visual.
- **I3 states** — hover/press/disabled handled where meaningful.

### V — Verification
- **V1 tests** — has tests covering its own behaviour.

## Output format

One block per widget, nothing else:

```
## <widget>
T1 PASS  colours via ctx.tc(theme.colors.*) — L102-118
T2 FAIL  title_size hardcoded 11.0 — L40; theme.typography.body_large is 17
S1 N/A   leaf with no surface of its own
A1 PASS  Role::Button + label — L103
X1 FAIL  width estimated `len()*size*0.6` — L93
...
NOTES: <anything that needs judgement, or an UNKNOWN and how to settle it>
```

Verdicts: `PASS` / `FAIL` / `N/A` / `UNKNOWN`. Every non-PASS carries a
reason and a line number.
