# Phase 28 — Real TextInput, Real IME, Forms Wiring (D112)

> Status: Scoped, not started.
> Started: —
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
