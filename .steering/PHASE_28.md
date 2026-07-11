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

### Step 2 — `TextArea` (multi-line)
New widget, same editing primitives as Step 1 plus line-wrapping (reuse `rosace_text::word_wrap`, the one real consumer already wired into `Text`) and vertical cursor movement (up/down crossing wrapped lines).

Exit: a real running app's multi-line field wraps text correctly and up/down arrow keys move the cursor across wrapped lines, verified live.

### Step 3 — Real OS IME
Desktop: winit's `WindowEvent::Ime` (`Enabled`/`Preedit`/`Commit`/`Disabled`) wired into `rosace-platform`, replacing `NoopIme` with a real `ImeHandler` impl that updates `ImeComposition`'s preedit state and renders an underlined composition string at the cursor (standard IME UX). Mobile: `UITextInput` (iOS) / `InputConnection` (Android) reachable only through the D106 FFI bridge — new capability, same three-piece shape (`capability.rs`'s camera pattern) as Phase 29's lifecycle/push work.

Exit: typing CJK text (e.g. via a Pinyin IME on macOS) into a real running app shows correct preedit composition and commits the right characters — verified live on a real OS IME, not simulated.

### Step 4 — Wire `rosace-forms`
`TextInput`/`TextArea` gain an optional `.field(Field)` binding to `rosace-forms`'s existing `Field`/`Validator` types (reused as-is — no rewrite unless a real integration blocker surfaces). Validation errors render inline (reuse existing `Semantics`/error-styling conventions). `Form::submit()` becomes reachable from a real button press.

Exit: a real running app's form shows a validation error inline when a required field is empty, clears it when corrected, and a submit button is disabled/enabled based on `Form`'s validity state — verified live.

## Sequencing

Steps 1→2 are sequential (TextArea reuses Step 1's editing primitives). Step 3 (IME) can start once Step 1 lands — independent of Step 2. Step 4 (Forms) needs Step 1 at minimum (a `Field` needs a real editable input to bind to).

**Explicit note carried from Known Issue #11 (D111)**: if any step needs a *list* of `TextInput`s (e.g. a dynamic form), do NOT build it on `ListView`'s positional-slot allocation without first confirming per-row identity is stable — that's the exact bug class that broke the image fade. Flag this explicitly if it comes up rather than rediscovering it live.

## Migration Rule

`TextInput`'s existing builder API (`.value()`, `.placeholder()`, `.focused()`, `.obscure()`, `.width()`, `.height()`) is unchanged — apps using it today keep working, they just gain real editing for free once Step 1 lands. `TextArea` and `.field()` are additive.
