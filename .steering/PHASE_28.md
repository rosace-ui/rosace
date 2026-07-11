# Phase 28 — Real TextInput, Real IME, Forms Wiring (D112)

> Status: Step 1 landed. Steps 2-4 not started.
> Started: 2026-07-12
> Completed: —
> Decision: **D112** — `TextInput` gains real keyboard editing, a new
> `TextArea` (multi-line), real OS IME composition (replacing
> `rosace-ime`'s `NoopIme`), and `rosace-forms` gets wired to both.
> Sequenced after Phase 27 (GPU rendering) per user priority.

## Why This Phase

Checked the actual code, not the plan: `TextInput` (`rosace-widgets/src/tree/text_input.rs`, 92 lines) is paint-only — draws a box, placeholder/value text, and a static cursor bar if `focused` (a plain bool field you set manually). `rosace/src/engine.rs`'s `KeyDown` handling only drives Tab focus-cycling — no key event ever inserts, deletes, or moves a cursor. **You cannot type into it in a running app today.** `rosace-ime` (`ImeHandler`/`NoopIme`/`ImeComposition`/`ImeEvent`/`ImeState`, 546 lines) models the *shape* of IME composition but `NoopIme` is the only implementation — `PHASE_8.md` explicitly deferred real OS IME to v1.0. `rosace-forms` (`field.rs`/`validator.rs`/`form.rs`, 503 lines) has zero references anywhere outside its own crate — built, never wired, the same pattern already found in `ScrollView`/`Navigator`/`ImageCache`.

The user's original priority was "TextInput/**Forms**" (not just editing) — this phase is that full scope.

## Out of Scope (deliberately, not silently dropped)

- **Rich text editing** (mixed styles within one input). `TextArea`/`TextInput` here are plain-text only; `rosace-text`'s `RichText`/`TextSpan` wiring is Phase 32's job, for the read-only `Text` widget, not editable inputs.
- **Autocomplete/suggestion UI.** A real feature in its own right, needs its own overlay/positioning design — not bundled into basic editing.
- **Spellcheck.** Platform-native spellcheck hookup (`NSSpellChecker`/Android equivalent) is a capability-bridge candidate for a future phase, not this one.

## Steps

### Step 1 — Real keyboard editing for `TextInput`
Cursor position + selection range become real per-node state (D091 — same discipline as everything else, stored on the render-tree node, not a widget-owned field). `rosace/src/engine.rs`'s `KeyDown`/`TZR_EVENT_TEXT` dispatch routes to the focused `TextInput`: character insert, Backspace/Delete, arrow-key cursor movement, Shift+arrow selection, Home/End, Cmd/Ctrl+A select-all, Cmd/Ctrl+C/V/X via the existing `rosace-clipboard` (already real — shells to `pbcopy`/`pbpaste` on macOS). Blinking cursor becomes real (currently just a static bar).

Exit: a real running app lets a user click into a `TextInput`, type, use arrow keys and Backspace, select text with Shift+arrows, and copy/paste — verified live, not just compiled.

**Landed 2026-07-12.** Architecture: cursor/selection are real persistent
per-node state (`TreeNode::text_edit: TextEditState`, new `rosace-widgets`
module `tree::text_edit` — pure, unit-tested char-indexed operations:
`insert_str`/`insert_char`/`backspace`/`delete_forward`/`move_left`/
`move_right`/`move_home`/`move_end`/`select_all`/`selected_text`, 21 unit
tests). `TextInput` stays a CONTROLLED component (`.value()`/`.on_change()`,
same convention as `Slider`/`Switch`) — the render-tree node persists only
the ephemeral caret/selection, not the value itself. A widget declares
itself onto its node each paint via a new `TreeNode::editable:
Option<EditableDecl>` (value/rect/multiline/obscure/on_change) — the
engine's key/click dispatch reads this directly rather than through a
captured closure, because computing a click position needs `FontCache`
and mutating the caret needs `Rc<RefCell<RenderTree>>`, and BOTH fail
`on_press_at`'s `Send + Sync` bound. `PaintCtx` gained `focus_node()` /
`focus_node_seeded()` (a per-node-position auto-created `FocusNode`,
mirroring `scroll_controller()`'s "zero wiring by default" precedent —
`TextInput::new()` is Tab-reachable and click-focusable with no explicit
`FocusNode` required) and `register_editable()`/`text_edit()`.
`rosace-platform`'s `Key` enum gained `Delete`/`Home`/`End` (wired in
`app.rs`'s winit `KeyCode` match). `rosace/src/engine.rs` gained
`ctrl_held`/`meta_held` (mirrors `shift_held`), `focused_editable()` +
`commit_text_edit()` helpers, and dispatch arms for `Text` (literal
insertion — NOT `KeyDown{Char}`, which fires alongside `Text` for every
plain letter and would double-insert; `KeyDown{Char}` is reserved for
Cmd/Ctrl-modified shortcut letters), Backspace, Delete, ArrowLeft/Right
(+Shift extends selection), Home/End (+Shift), and Cmd/Ctrl+A/C/X/V
(select-all / copy / cut / paste via the real `rosace-clipboard`
`SystemClipboard`, gated on EITHER Ctrl or Meta — not OS-branched).

**A real bug found and fixed by the test suite, not by inspection:**
click-to-focus initially called `FocusNode::request()` directly, which
sets only that ONE node's own reactive `Atom<bool>` — but
`FocusManager.focused` (the id `focused_editable()` reads to find the
dispatch target) is separate state, only ever updated by
`FocusManager`'s own methods. The very first integration-test run typed
into nothing (atom stayed empty) because of this gap. Fixed by adding
real `FocusManager::focus_specific(id)` (release-old/request-new, same
invariant as `activate()`, but jumping to a KNOWN id instead of
stepping relative) and `FocusManager::blur()` (release + clear) to
`rosace-a11y`, with their own unit tests — this is now the one correct
way to focus/blur a specific node, not a `TextInput`-only patch.

**Verification**: OS-level synthetic input (CGEvent mouse/keyboard
injection into the running app) was attempted and confirmed blocked —
same Accessibility-permission gap documented earlier in this project's
history (a real click landed exactly on the field's declared rect,
window frontmost, produced no observable effect). Real verification
instead: 10 new integration tests in `rosace/src/engine.rs` drive the
REAL `FrameEngine` with synthetic `InputEvent`s through the exact
production dispatch path — click focuses + typed text reaches a real
app-owned `Atom<String>` via `on_change`; typing before any click is
correctly dropped; Backspace/Delete; cursor-tracked mid-string insert
(not just append); Shift+arrow select-then-replace; Cmd+A AND Ctrl+A
select-all; blank-space click blurs; Tab moves focus from a first
`TextInput` to a second and types into the CORRECT one; a full cut→paste
round-trip through the REAL system clipboard (saves/restores whatever
was there so the test has no lasting side effect). Plus 6 new
`FocusManager` unit tests for `focus_specific`/`blur` directly. All pass;
full `cargo build --workspace --all-targets` clean; full
`cargo test --workspace` clean (the one `rosace-state` failure under
parallel execution is the pre-existing documented Known Issue #8 flake —
confirmed passing 15/15 in isolation, unrelated to this change).
Additionally screenshot-verified live in two real running apps
(`text_input_demo` — two atom-bound fields, pre-filled value renders
correctly; a throwaway focus-visual check — confirms the real violet
focus ring and blinking caret actually paint, not just that state
flags flip).

**Known, stated simplification (not a silent gap)**: click places the
caret at the END of the value, not at the clicked glyph. Precise
click→position needs font metrics (`FontCache::measure_text`), and
`FontCache` is `!Sync` (internal `RefCell` glyph/route caches) — it
cannot cross into `on_press_at`'s `Send + Sync`-bound closure, and the
engine's click dispatch (which CAN reach the render tree) doesn't have
`FontCache` either. A real fix needs either a `Sync`-safe metrics
snapshot or routing click positioning through the same paint-time path
glyph layout already uses — scoped as a real follow-up, not bundled in
here. Mouse-drag text selection is also not implemented (the exit bar
only requires Shift+arrow selection, which is real and tested).

> **Steps 2+ REWRITTEN 2026-07-12 per D116** (the text-editing
> architecture decision — read it first; it holds the "why" for every
> seam below). The original Step 2/3/4 scopes survive as Steps 4/6/8.
> The user's standing framing applies in full force here: this is not
> an MVP — no delivery-date pressure, the exit bars are the schedule.

### Step 2 — Edit core: transactions, multi-range selection, commands, undo (D116 layers 2+4)
Behavior-preserving upgrade of `tree::text_edit` — Step 1's tests keep
passing throughout:
- `Transaction` (`Vec<(range, replacement)>`, applied atomically,
  invertible) becomes the ONLY way text changes; Step 1's pure ops
  become transaction builders.
- `Selection` becomes `Vec<SelectionRange { anchor, head, affinity }>`
  (single caret = one element; multi-cursor is a data-model citizen NOW
  so a future code-editor phase is UI work, not a rewrite).
- **Undo/redo**: per-field stack of inverse transactions, persistent on
  the node (D091), with edit coalescing (consecutive typing = one undo
  unit, standard everywhere) and Cmd/Ctrl+Z / Shift+Cmd/Ctrl+Z commands.
- `Command` enum + default keymap between key events and the core;
  engine dispatch translates `Key`→`Command`→transaction. Adds word-wise
  ops: Alt/Ctrl+arrows, Alt/Ctrl+Backspace (word boundaries via
  `unicode-segmentation`, approved in D116).
- Cursor movement/deletion boundaries tighten from chars to **grapheme
  clusters** (é, 🇮🇳, family emoji move/delete as one unit).
- **`EditController`** (D101 `ScrollController` precedent):
  `.controller()` on both widgets — `replace_range`/`insert_at_cursor`/
  `set_selection`/`select_all`/`undo`/`redo` for programmatic editing
  (the markdown-toolbar-Bold-button API).

Exit: all Step 1 integration tests still green untouched; new headless
tests prove undo/redo round-trips (incl. coalescing), word-wise ops,
grapheme-safe movement over emoji/accents, and an `EditController`
wrapping a selection in `**` like a real toolbar would.

### Step 3 — `TextLayoutSnapshot`: click-to-glyph, drag selection (D116 layer 3)
The keystone seam. During paint, the widget stores plain-data geometry
(line boxes, per-boundary caret x positions) on its node; engine
dispatch queries it with no `FontCache` access — dissolving Step 1's
documented `!Sync` wall and its caret-to-end simplification.
- Click places the caret at the clicked grapheme boundary (nearest-half
  rule, the universal convention).
- **Mouse drag selects**; double-click selects word; triple-click
  selects line. Selection rendering already exists from Step 1.
- `TextInput` horizontally scrolls its content to keep the caret
  visible in overflow (snapshot supplies position→x).

Exit: headless engine tests — synthetic MouseDown at a mid-string x
places the caret at the correct index (assert exact index for a known
string/font); down+move+up produces the exact expected selection;
double/triple-click select word/line. Live demo screenshot shows a
mid-string caret and a drag selection.

### Step 4 — `TextArea` (multi-line, virtualized) (original Step 2 + D116)
New widget on the Step 2/3 core: line wrapping (reuse
`rosace_text::word_wrap`), Enter inserts newline, up/down movement
across wrapped lines with **goal-column memory** (vertical moves through
short lines remember the departed x) and wrap-boundary **affinity**;
paints only viewport-visible lines (the existing scroll machinery;
line-index makes this cheap) so a large document doesn't tank paint.
Known Issue #11 note carries: a LIST of TextAreas must not rely on
ListView positional slots.

Exit (original bar kept + additions): wrapping correct and up/down
crosses wrapped lines with goal-column behavior, verified live; a
several-thousand-line value scrolls with only visible lines painted
(assert via paint-command counts headlessly).

### Step 5 — Spans + cursor customization: the styling seams (D116 layer 5)
- **`SpanSource` hook**: `.spans(fn(&str, changed_range) -> Vec<Span>)`
  on `TextInput`/`TextArea` — the widget paints value text as styled
  runs (color/weight per span), re-tokenizing incrementally from
  transaction ranges. THE markdown/syntax-highlighting seam: the app
  brings the tokenizer; the core never learns what markdown is.
- **Decorations**: range-keyed background layers (selection highlight
  generalized; search-match highlights; Step 6 reuses this for the IME
  preedit underline).
- **`CursorStyle`**: we already paint the caret ourselves, so expose it —
  width, color, corner radius, blink rate, shape (`Bar`/`Block`/
  `Underline`) and `Custom` (app-supplied painter — any `DrawCommand`s,
  an icon, even a Phase 27 shader). Theme-level default + per-field
  override. (Direct answer to the user's cursor question: yes — planned
  exactly here.)

Exit: a demo `TextArea` with a toy `**bold**`-highlighting `SpanSource`
shows live styled spans while editing (screenshot); a block-cursor and a
custom-painter cursor render (screenshot); incremental invalidation
proven headlessly (tokenizer called with the changed range, not the
whole string, on a small edit).

### Step 6 — Real OS IME (original Step 3, now on D116 seams)
Desktop: winit `WindowEvent::Ime` (`Enabled`/`Preedit`/`Commit`/
`Disabled`) into `rosace-platform`; preedit = a **provisional
transaction** + underline **decoration** (Step 5) at the caret; commit
finalizes it through the same pipeline as typing. Report the IME
candidate-window rect from the **snapshot** (Step 3) via
`set_ime_cursor_area`. Replace `NoopIme` as the original scope said.
Mobile: `UITextInput` (iOS) / `InputConnection` (Android) via the D106
FFI bridge (camera-capability three-piece shape); fix **Known Issue #15**
(missing `TZR_KEY_DELETE/_HOME/_END`) here; add **keyboard-type hints**
(email/numeric/URL — OS keyboard hints via winit + an FFI field, not
distinct widget types).

Exit (original bar kept): real CJK preedit + commit via a real macOS
Pinyin IME, verified live. Mobile IME exercised in the next real
`rsc run` mobile session alongside Phase 27's pending mobile sanity.

### Step 7 — Device-adaptive selection UX: context menu, touch handles
- **Context menu** (right-click desktop / long-press touch) with Cut/
  Copy/Paste/Select All — built on the existing overlay system, driving
  the same `Command`s as the keyboard shortcuts.
- **Touch selection**: tap positions caret (Step 3 snapshot), long-press
  selects word + shows **selection handles** (draggable via the existing
  positional-hit drag machinery) and a magnifier loupe (a Phase 27
  offscreen/shader job — the machinery exists).
- Right-click needs `MouseButton::Right` routing in engine dispatch
  (currently only Left is handled) — small, named here so it isn't a
  surprise.

Exit: desktop context menu verified live (screenshot: menu open over a
field, Paste inserts real clipboard content). Touch handles verified in
the mobile session with Step 6 (they share the FFI work); headless tests
for handle-drag → selection updates land with this step regardless.

### Step 8 — Wire `rosace-forms` (original Step 4, scope unchanged)
`.field(Field)` binding on both widgets; inline validation errors
(existing `Semantics`/error conventions); `Form::submit()` from a real
button. Plus **input filters** (`.filters()` — max length, character
classes) landing here since validation and filtering are one UX story.

Exit (original bar kept): required-field error appears/clears live;
submit button enables/disables on `Form` validity — verified live.

## Sequencing

2 → 3 → 4 strictly (core, then geometry, then multi-line consumes both).
5 needs 2 (transaction ranges drive invalidation) and benefits from 4.
6 needs 2 (provisional transactions) + 3 (cursor rect) + 5 (underline
decoration). 7 needs 3 (all pointer positioning) + 6 for the mobile
half. 8 needs only Step 1 and can interleave anytime after — it's the
natural "breather" step between the heavier ones.

**Explicit note carried from Known Issue #11 (D111)**: if any step needs
a *list* of editable fields (e.g. a dynamic form), do NOT build it on
`ListView`'s positional-slot allocation without first confirming
per-row identity is stable — that's the exact bug class that broke the
image fade. Flag it explicitly rather than rediscovering it live.

## Migration Rule

`TextInput`'s existing builder API (`.value()`, `.placeholder()`,
`.focused()`, `.obscure()`, `.width()`, `.height()`, `.on_change()`) is
unchanged through EVERY step above — Step 2's internal transaction
rewrite must not disturb it (its tests are the regression bar).
`TextArea`, `.controller()`, `.spans()`, `.cursor_style()`, `.filters()`,
`.field()` are all additive. `rosace-ime`'s public types are reused
where they fit (Step 6), per the original scope.

## What this phase still deliberately does NOT build (per D116)

Editable rich text/WYSIWYG; a code-editor-class widget (gutters,
folding, multi-cursor UI, huge-file virtualization — future phase ON
these seams; the data model for it lands in Step 2); BiDi caret
movement (v1.0 with D014); rope storage (behind the transaction seam;
trigger: real >~1MB documents); spellcheck; autocomplete; drag-and-drop
text. Each has a named home — none is silently dropped.
