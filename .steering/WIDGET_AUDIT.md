# Widget Audit vs the Quality Bar (2026-07-24)

Scored all widgets by interaction/motion/theming signals (see
`WIDGET_QUALITY_BAR.md`). Not every widget needs every item — a `Divider` has no
states. Grouped by what the widget IS.

## ✅ Premium — pass the bar (matured this session or already deep)
`button` · `switch` · `checkbox` · `slider` · `radio` · `text_input` ·
`text_area` (deep text stack). Partial-but-good: `fab`, `list_tile`, `stepper`,
`tab`, `expander`, `carousel`, `scroll_view`, `interactive_viewer`.

## ⚠️ Needs work — INTERACTIVE but thin (this audit's target list)
| Widget | Gap |
|---|---|
| `chip` | 59 lines, 0 hover/press/focus/disabled — selectable/deletable chip must have them |
| `segmented` | has tokens/anim but no hover/press/focus on segments |
| `rating_bar` | interactive stars, 0 hover/press (no hover-preview) |
| `search_bar` | 0 of everything incl. semantics — thin |
| `dropdown` | trigger + items lack hover/press/focus |
| `menu` | menu items need hover/press/keyboard highlight |
| `tabs` | tab strip lacks hover + animated indicator polish |
| `nav_rail` / `bottom_nav` | nav items need selected/hover state layers |
| `date_picker` / `time_picker` | day/time cells need hover/press/selected |

## 🔵 Display — simpler bar (mostly fine; polish only if noted)
`text` · `icon` · `image` · `avatar` · `badge` · `divider` · `progress_bar` ·
`circular_progress` · `skeleton` · `tooltip` · `card` · `snackbar` · `toast` ·
`dialog` · `sheet` · `drawer` · `app_bar` · `table` · `data_table` · `pressable`.
(Interactive sub-parts — a dialog's buttons, a snackbar's action — inherit
`Button`, so they're already premium.)

## ⬜ N/A — structural/layout (no interaction states by nature)
`column` `row` `stack` `grid` `wrap` `container` `padding` `spacer`
`aspect_ratio` `positioned` `scaffold` `list_view` `repaint_boundary` `hero`
`screen_transition_view` `overlay` `pointer` `custom_paint` `shader_paint`
`transform_layer` `rect_reader`.

## Plan
Work the ⚠️ list top-to-bottom in one stretch, each to the bar (states, motion
via `animate_channel`, tokens, focus, disabled, a11y), verify by eye, commit.
