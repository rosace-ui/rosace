# Date & Time Picker — spec (2026-07-24, user)

Both must be Android-Material style, fully stylable per-component, theme- &
animation-compliant (the skin/registry direction), and configurable.

## TimePicker — Material CLOCK DIAL (replaces the current spinner)
- **Clock dial** UI: a circular face, hour numbers (1–12) around the rim in
  Hour mode; minute ticks (labelled every 5) in Minute mode. A **hand** from
  the centre to the selected value with a **thumb** disc at its end; a centre dot.
- **Hour + Minute modes** with an AM/PM toggle. Header shows `HH : MM AM/PM`;
  tapping the hour or minute switches which the dial edits.
- **NO seconds.** (User: the second hand was a mistake and "miserable" — remove
  it. Keep `SimpleTime` = hour + minute only.)
- **Animated hand.** On launch it **animates to the current time** — starting
  the hand at 12:00 (top) and sweeping to the target — not popping into place.
  Later value changes also animate the hand.
- **Stylable per component** (all builders, theme-defaulted, overridable):
  dial/face circle color, hand color, thumb color, number (unselected) text
  color, selected-number text color, tick color, accent.
- Tap/drag on the dial selects; controlled (`value` + `on_change`), like the
  rest. Dial edit-mode (hour vs minute) is controlled too (a `TimeUnit` +
  `on_unit_change`) so it stays a stateless value type.

## DatePicker — same styling contract + RANGE + selection modes
- **Selection mode** (`.mode(...)`): **Single** (current) OR **Range**
  (start→end, with the in-between days highlighted as a band), possibly Multiple
  later. Configurable.
- **Range**: pick a start day, then an end day; render the start/end as filled
  circles and the days between as a connected range band; `on_range_change`.
- **Stylable per component** like the TimePicker: selected circle color, range
  band color, today-ring color, weekday-header color, day text colors
  (normal/selected/disabled/today), nav-arrow color, accent. All theme-defaulted.
- Keep the per-cell hover just added; add press feedback consistent with it.

## Shared
- Both animate transitions (month change slides/fades; hand sweeps) via
  `animate_channel`, honoring the theme animation config.
- Both remain composable inside a `Dialog` for the modal flow.
- Every color is a token by default and a builder override — ready for the
  skin registry when it lands.
