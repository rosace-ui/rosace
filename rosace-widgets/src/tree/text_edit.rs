//! The text-editing core (D112/Phase 28 Step 1, restructured under D116).
//!
//! Five layered seams, per D116, all living in this one module except the
//! platform keymap (which needs `rosace_platform::Key` — a lower layer
//! this crate doesn't depend on — so the Key→Command translation lives in
//! `rosace/src/engine.rs`, which already sees both):
//!
//! 1. **Document** — a plain `String`, app-owned (`EditableDecl::value`),
//!    mutated ONLY through [`Transaction`]s below.
//! 2. **Edit core** — [`Transaction`] (invertible), [`Selection`] (a list
//!    of ranges — multi-cursor is a data-model citizen now, a UI feature
//!    later), undo/redo (a per-field stack of inverse transactions with
//!    typing coalesced into one unit), grapheme/word boundaries via
//!    `unicode-segmentation`.
//! 3. **Layout seam** — `TextLayoutSnapshot` is Step 3's job; not here yet.
//! 4. **Behavior** — [`Command`], the abstract vocabulary a keymap
//!    translates key events into (`rosace/src/engine.rs` owns the actual
//!    keymap and clipboard I/O; [`apply_command`] here executes the
//!    non-clipboard commands).
//! 5. **Render** — [`EditController`] is the app-facing programmatic
//!    handle (D101 `FocusNode`/`ScrollController` precedent); styling
//!    (`SpanSource`/`CursorStyle`) is Step 5's job.
//!
//! Positions are CHAR indices (`str::chars()` count), not byte indices —
//! simple, stable String-slicing math. Grapheme-cluster correctness
//! (combining marks, ZWJ emoji, flag pairs treated as one editable unit)
//! comes from snapping every cursor/selection boundary produced by the ops
//! below to real grapheme boundaries — never from redefining the
//! coordinate space itself.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rosace_core::types::Rect;
use unicode_segmentation::UnicodeSegmentation;

// ─────────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────────

/// Which side of a wrap/grapheme boundary a caret visually prefers.
/// Unused by single-line `TextInput`; carried from day one (D116) so
/// Step 4's wrapped up/down movement and v1.0's BiDi caret don't need a
/// `Selection` rewrite to add it later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Affinity {
    #[default]
    Upstream,
    Downstream,
}

/// One selection range: `anchor` is where the selection started,
/// `head` is the live end (where the caret glyph renders). `anchor ==
/// head` is a plain collapsed caret — the overwhelmingly common case for
/// `TextInput`/`TextArea` today; multiple `SelectionRange`s (multi-cursor)
/// is a real, supported shape nothing here forbids, reserved for a future
/// code-editor-class widget (D116 — not built by this phase).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionRange {
    pub anchor: usize,
    pub head: usize,
    pub affinity: Affinity,
}

impl SelectionRange {
    pub fn collapsed_at(pos: usize) -> Self {
        Self { anchor: pos, head: pos, affinity: Affinity::default() }
    }
    pub fn collapsed(&self) -> bool {
        self.anchor == self.head
    }
    /// `(start, end)` with `start <= end` — the shape every string-slicing
    /// call site wants, regardless of which direction the user selected in.
    pub fn normalized(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }
}

/// A list of [`SelectionRange`]s — NEVER empty (enforced by keeping the
/// backing `Vec` private behind constructors that always seed one range).
/// `TextInput`/`TextArea` only ever populate the primary (last) range;
/// the list shape exists so a future multi-cursor widget is additive, not
/// a rewrite (D116).
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    ranges: Vec<SelectionRange>,
}

impl Selection {
    pub fn single(pos: usize) -> Self {
        Self { ranges: vec![SelectionRange::collapsed_at(pos)] }
    }
    pub fn range(anchor: usize, head: usize) -> Self {
        Self { ranges: vec![SelectionRange { anchor, head, affinity: Affinity::default() }] }
    }
    /// The primary (most-recently-active) range — for single-cursor
    /// widgets, the only one that exists.
    pub fn primary(&self) -> &SelectionRange {
        self.ranges.last().expect("Selection is never empty")
    }
    pub fn primary_range(&self) -> (usize, usize) {
        self.primary().normalized()
    }
    pub fn ranges(&self) -> &[SelectionRange] {
        &self.ranges
    }
}

impl Default for Selection {
    fn default() -> Self {
        Selection::single(0)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Transaction
// ─────────────────────────────────────────────────────────────────────────

/// One atomic edit: the chars in `range` (char indices, `[start, end)`,
/// against the string this edit is applied to) are replaced by
/// `replacement`.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub range: (usize, usize),
    pub replacement: String,
}

/// A set of [`Edit`]s applied atomically. Multiple edits must target
/// DISJOINT, non-overlapping ranges of the string being applied to (the
/// "type the same char at every cursor" shape a future multi-cursor
/// widget needs) — [`Transaction::apply`] applies them highest-range-first
/// internally so earlier (lower) ranges' indices never shift under later
/// ones, and produces a real inverse: applying the inverse to the result
/// exactly reconstructs the input. This IS the undo mechanism (D116) —
/// nothing else computes or stores a diff.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Transaction {
    pub edits: Vec<Edit>,
}

impl Transaction {
    pub fn single(range: (usize, usize), replacement: impl Into<String>) -> Self {
        Transaction { edits: vec![Edit { range, replacement: replacement.into() }] }
    }

    /// Apply to `value`, returning the new value and this transaction's
    /// inverse (an undo entry — see the struct doc).
    pub fn apply(&self, value: &str) -> (String, Transaction) {
        let mut edits = self.edits.clone();
        edits.sort_by(|a, b| b.range.0.cmp(&a.range.0)); // highest start first

        let mut result = value.to_string();
        let mut inverse_edits = Vec::with_capacity(edits.len());
        for e in &edits {
            let bs = char_byte_offset(&result, e.range.0);
            let be = char_byte_offset(&result, e.range.1);
            let removed = result[bs..be].to_string();
            let mut next = String::with_capacity(result.len() - (be - bs) + e.replacement.len());
            next.push_str(&result[..bs]);
            next.push_str(&e.replacement);
            next.push_str(&result[be..]);
            let new_end = e.range.0 + char_count(&e.replacement);
            inverse_edits.push(Edit { range: (e.range.0, new_end), replacement: removed });
            result = next;
        }
        (result, Transaction { edits: inverse_edits })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Char/grapheme/word boundary helpers
// ─────────────────────────────────────────────────────────────────────────

/// Number of chars in `s` — the coordinate space every position here uses.
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// Byte offset of char index `idx` in `s` (clamped to `s.len()` past the
/// end) — the bridge from char-indexed positions to `&str` slicing.
pub fn char_byte_offset(s: &str, idx: usize) -> usize {
    s.char_indices().nth(idx).map(|(b, _)| b).unwrap_or(s.len())
}

/// Char-index positions of every grapheme-cluster boundary in `s`,
/// including 0 and `char_count(s)`. A combining-mark sequence or a ZWJ
/// emoji sequence (family emoji, flag pairs) collapses to ONE boundary
/// step, not one per codepoint — this is what makes movement/deletion
/// grapheme-correct (D116) without changing the char-index coordinate
/// space everything else here uses.
///
/// O(n) per call, recomputed fresh each time — fine for the field-length
/// text this phase's widgets hold. Step 4 (large `TextArea` documents)
/// should revisit with a cached/incremental structure rather than calling
/// this per keystroke on a multi-thousand-line value.
pub fn grapheme_boundaries(s: &str) -> Vec<usize> {
    let mut bounds = Vec::with_capacity(s.len() + 1);
    bounds.push(0usize);
    let mut char_idx = 0usize;
    for g in s.graphemes(true) {
        char_idx += g.chars().count();
        bounds.push(char_idx);
    }
    bounds
}

/// The nearest grapheme boundary strictly before `pos` (0 if none).
pub fn prev_grapheme_boundary(s: &str, pos: usize) -> usize {
    grapheme_boundaries(s).into_iter().rev().find(|&b| b < pos).unwrap_or(0)
}

/// The nearest grapheme boundary strictly after `pos` (the string's end
/// if none).
pub fn next_grapheme_boundary(s: &str, pos: usize) -> usize {
    let bounds = grapheme_boundaries(s);
    bounds.iter().copied().find(|&b| b > pos).unwrap_or_else(|| *bounds.last().unwrap())
}

/// Char-index boundaries between word/non-word runs (`split_word_bounds`),
/// including 0 and `char_count(s)`.
fn word_bound_boundaries(s: &str) -> Vec<(usize, bool)> {
    // (char_idx_start, is_word) for each run, plus a trailing sentinel.
    let mut out = Vec::new();
    let mut char_idx = 0usize;
    for w in s.split_word_bounds() {
        let is_word = w.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false);
        out.push((char_idx, is_word));
        char_idx += char_count(w);
    }
    out.push((char_idx, false)); // sentinel end
    out
}

/// Word-left: skip any whitespace/punctuation immediately to the left,
/// then land at the start of the previous word run (or 0). Standard
/// Alt/Ctrl+Left convention.
pub fn prev_word_boundary(s: &str, pos: usize) -> usize {
    let runs = word_bound_boundaries(s);
    // Find the run containing (or immediately before) `pos`.
    let mut idx = runs.len().saturating_sub(1);
    for i in (0..runs.len() - 1).rev() {
        if runs[i].0 < pos {
            idx = i;
            break;
        }
    }
    // Walk left, skipping non-word runs, to the start of the previous
    // word run strictly before `pos`.
    let mut i = idx;
    loop {
        let (start, is_word) = runs[i];
        if start < pos && is_word {
            return start;
        }
        if i == 0 {
            return 0;
        }
        i -= 1;
    }
}

/// Word-right: if the cursor sits inside (or at the start of) a word run,
/// land at THAT run's end; otherwise skip forward past whitespace/
/// punctuation to the next word run's end (or the string's end). Standard
/// Alt/Ctrl+Right convention.
pub fn next_word_boundary(s: &str, pos: usize) -> usize {
    let runs = word_bound_boundaries(s); // ascending starts, sentinel (n, false) last
    let n = char_count(s);

    // The run containing `pos`: the last run whose start <= pos.
    let mut i = 0;
    while i + 1 < runs.len() && runs[i + 1].0 <= pos {
        i += 1;
    }

    if runs[i].1 {
        let end = runs.get(i + 1).map(|&(st, _)| st).unwrap_or(n);
        if end > pos {
            return end;
        }
    }
    // In whitespace (or already at a word run's end) — advance to the
    // next word run's end.
    let mut j = i + 1;
    while j < runs.len() {
        if runs[j].1 {
            return runs.get(j + 1).map(|&(st, _)| st).unwrap_or(n);
        }
        j += 1;
    }
    n
}

// ─────────────────────────────────────────────────────────────────────────
// EditableDecl (D112) — declared onto the render-tree node each paint
// ─────────────────────────────────────────────────────────────────────────

/// What an editable widget declares onto its render-tree node each paint
/// (D112) — cleared and re-declared every repaint, like `hits`/`scrolls`.
/// The engine's key/click dispatch finds this via the render tree, NOT a
/// captured closure: computing a click->glyph position needs `FontCache`
/// (`!Sync`, cannot cross into a `Send + Sync` closure), and mutating the
/// caret needs `Rc<RefCell<RenderTree>>` (`!Send`, same problem) — both
/// are reachable from engine.rs, neither is reachable from a plain
/// `Arc<dyn Fn + Send + Sync>` hit callback.
pub struct EditableDecl {
    pub value: String,
    /// World-space rect this paint, for click-to-focus hit testing.
    pub rect: Rect,
    pub multiline: bool,
    pub obscure: bool,
    pub on_change: Arc<dyn Fn(String) + Send + Sync>,
    /// Optional programmatic handle (D116) — `None` for the common case
    /// (an app that only uses `.value()`/`.on_change()`).
    pub controller: Option<EditController>,
}

// ─────────────────────────────────────────────────────────────────────────
// TextEditState — persistent per-node chrome (D091)
// ─────────────────────────────────────────────────────────────────────────

const COALESCE_WINDOW_SECS: f32 = 0.5;

#[derive(Clone, Debug, PartialEq)]
struct UndoEntry {
    /// Applying this to the value AT UNDO TIME reconstructs the value
    /// from before the edit this entry records.
    inverse: Transaction,
    /// The selection to restore after undoing.
    selection_before: Selection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CoalesceInfo {
    at: f32,
    /// The char index right after the group's most recent insertion —
    /// the next insert must start exactly here (no intervening move) to
    /// extend the group instead of starting a new undo entry.
    cursor_after: usize,
}

/// Persistent per-node editing state (D091): selection + undo/redo
/// history, NOT the text value itself. The value stays app-owned,
/// reported via `EditableDecl::on_change` — the same controlled-component
/// convention every other stateful widget here uses (`Slider`, `Switch`,
/// `Checkbox`). Survives a rebuild with the same value untouched — a
/// widget rebuilding keeps its caret position and undo history.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditState {
    pub selection: Selection,
    /// `anim_clock()` timestamp of the last edit/move — the caret blink
    /// resets to solid-on from this point, so typing/navigating always
    /// reads as responsive instead of possibly mid-blink-invisible.
    pub last_edit_at: f32,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    /// Set after a coalescible (plain, no-selection) insertion; consumed
    /// by the next insertion to decide whether to extend the top undo
    /// entry instead of pushing a new one. Any non-coalescing op
    /// (deletion, movement, selection change) clears it.
    coalesce: Option<CoalesceInfo>,
}

impl Default for TextEditState {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            last_edit_at: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            coalesce: None,
        }
    }
}

impl TextEditState {
    /// The primary caret's live position — sugar over `.selection`, kept
    /// for the common single-cursor read (rendering the caret glyph).
    pub fn cursor(&self) -> usize {
        self.selection.primary().head
    }
    /// Normalized `(start, end)` char range, or `None` when the primary
    /// selection is collapsed (no active selection).
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let r = self.selection.primary();
        if r.collapsed() { None } else { Some(r.normalized()) }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
    /// Set the selection directly, preserving undo/redo history — the
    /// primitive behind `EditController::set_selection` (a pure
    /// selection change is not an edit, so it must not touch the undo
    /// stack, but its private fields aren't reachable via struct-update
    /// syntax from outside this module).
    pub fn with_selection(&self, selection: Selection, now: f32) -> TextEditState {
        moved(self, selection, now)
    }
}

/// Apply `txn` to `value` and fold the result into a NEW [`TextEditState`]
/// (functional style, matching every op below): records the inverse onto
/// the undo stack — coalescing into the existing top entry when
/// `coalesce_key` (the edit's start position) matches the pending group's
/// end and the coalesce window hasn't elapsed — clears the redo stack
/// (any real edit invalidates future redo), and sets `new_selection`.
fn apply_and_record(
    value: &str,
    state: &TextEditState,
    txn: Transaction,
    new_selection: Selection,
    now: f32,
    coalesce_key: Option<usize>,
) -> (String, TextEditState) {
    let (new_value, inverse) = txn.apply(value);

    let can_coalesce = matches!(
        (coalesce_key, &state.coalesce),
        (Some(start), Some(info)) if start == info.cursor_after && (now - info.at) < COALESCE_WINDOW_SECS
    );

    let mut undo_stack = state.undo_stack.clone();
    if can_coalesce {
        if let (Some(top), Some(new_edit)) =
            (undo_stack.last_mut().and_then(|e| e.inverse.edits.first_mut()), inverse.edits.first())
        {
            // Widen the existing group's "delete this range" inverse to
            // also cover the newly inserted text.
            top.range.1 = new_edit.range.1;
        }
    } else {
        undo_stack.push(UndoEntry { inverse, selection_before: state.selection.clone() });
    }

    let coalesce = coalesce_key.map(|_| CoalesceInfo { at: now, cursor_after: new_selection.primary().head });

    let ns = TextEditState {
        selection: new_selection,
        last_edit_at: now,
        undo_stack,
        redo_stack: Vec::new(),
        coalesce,
    };
    (new_value, ns)
}

// ─────────────────────────────────────────────────────────────────────────
// Content-mutating ops — transaction builders (D116: "Step 1's pure ops
// become transaction builders")
// ─────────────────────────────────────────────────────────────────────────

/// Insert `text` at the cursor, replacing any active selection first.
/// Coalesces with immediately-preceding plain insertions into one undo
/// unit (D116) — only when there was no active selection at insert time;
/// replacing a selection always starts a fresh undo entry.
pub fn insert_str(value: &str, state: &TextEditState, text: &str, now: f32) -> (String, TextEditState) {
    let (start, end) = state.selection.primary_range();
    let txn = Transaction::single((start, end), text);
    let new_cursor = start + char_count(text);
    let coalesce_key = if start == end { Some(start) } else { None };
    apply_and_record(value, state, txn, Selection::single(new_cursor), now, coalesce_key)
}

/// Insert one character — sugar over [`insert_str`] for the common case
/// (`InputEvent::Text` delivers one `char` at a time).
pub fn insert_char(value: &str, state: &TextEditState, ch: char, now: f32) -> (String, TextEditState) {
    let mut buf = [0u8; 4];
    insert_str(value, state, ch.encode_utf8(&mut buf), now)
}

/// Replace an EXPLICIT `[start, end)` range — the primitive behind
/// `EditController::replace_range`, independent of wherever the caret
/// currently is. Never coalesces (a programmatic edit is its own event).
pub fn replace_range(value: &str, state: &TextEditState, start: usize, end: usize, text: &str, now: f32) -> (String, TextEditState) {
    let n = char_count(value);
    let (s, e) = (start.min(n), end.min(n));
    let (s, e) = (s.min(e), s.max(e));
    let txn = Transaction::single((s, e), text);
    let new_cursor = s + char_count(text);
    apply_and_record(value, state, txn, Selection::single(new_cursor), now, None)
}

/// Backspace: delete the selection if one is active, else the grapheme
/// cluster before the cursor (a combining-mark sequence or ZWJ emoji
/// disappears in ONE press, not one per codepoint). No-op at position 0
/// with no selection (still bumps `last_edit_at`, resetting the blink,
/// matching a real editor's "the caret flashes even on a no-op key").
pub fn backspace(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    let (start, end) = state.selection.primary_range();
    if start != end {
        let txn = Transaction::single((start, end), "");
        return apply_and_record(value, state, txn, Selection::single(start), now, None);
    }
    if start == 0 {
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, ..state.clone() });
    }
    let prev = prev_grapheme_boundary(value, start);
    let txn = Transaction::single((prev, start), "");
    apply_and_record(value, state, txn, Selection::single(prev), now, None)
}

/// Forward delete: symmetric with [`backspace`], grapheme-cluster aware.
pub fn delete_forward(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    let (start, end) = state.selection.primary_range();
    if start != end {
        let txn = Transaction::single((start, end), "");
        return apply_and_record(value, state, txn, Selection::single(start), now, None);
    }
    let n = char_count(value);
    if start >= n {
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, ..state.clone() });
    }
    let next = next_grapheme_boundary(value, start);
    let txn = Transaction::single((start, next), "");
    apply_and_record(value, state, txn, Selection::single(start), now, None)
}

/// Delete the word before the cursor (Alt/Ctrl+Backspace) — deletes an
/// active selection instead, if one exists, same convention as
/// [`backspace`].
pub fn delete_word_back(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    let (start, end) = state.selection.primary_range();
    if start != end {
        let txn = Transaction::single((start, end), "");
        return apply_and_record(value, state, txn, Selection::single(start), now, None);
    }
    let prev = prev_word_boundary(value, start);
    if prev == start {
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, ..state.clone() });
    }
    let txn = Transaction::single((prev, start), "");
    apply_and_record(value, state, txn, Selection::single(prev), now, None)
}

/// Delete the word after the cursor (Alt/Ctrl+Delete).
pub fn delete_word_forward(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    let (start, end) = state.selection.primary_range();
    if start != end {
        let txn = Transaction::single((start, end), "");
        return apply_and_record(value, state, txn, Selection::single(start), now, None);
    }
    let next = next_word_boundary(value, start);
    if next == start {
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, ..state.clone() });
    }
    let txn = Transaction::single((start, next), "");
    apply_and_record(value, state, txn, Selection::single(start), now, None)
}

// ─────────────────────────────────────────────────────────────────────────
// Movement / selection ops — never touch the undo stack (not content
// edits), always clear a pending coalesce group (an intentional move
// must not let a later insertion silently merge with an unrelated one).
// ─────────────────────────────────────────────────────────────────────────

fn moved(state: &TextEditState, selection: Selection, now: f32) -> TextEditState {
    TextEditState { selection, last_edit_at: now, coalesce: None, ..state.clone() }
}

/// Move left one grapheme cluster. Without `extend`, an active selection
/// first COLLAPSES to its start (matches every desktop text field's
/// convention — Left doesn't step from wherever the caret glyph renders).
pub fn move_left(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let sel = state.selection.primary();
    if !extend && !sel.collapsed() {
        return moved(state, Selection::single(sel.normalized().0), now);
    }
    let prev = prev_grapheme_boundary(value, sel.head);
    let new_sel = if extend { Selection::range(sel.anchor, prev) } else { Selection::single(prev) };
    moved(state, new_sel, now)
}

/// Move right one grapheme cluster (collapses an active selection to its
/// end first, symmetric with [`move_left`]).
pub fn move_right(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let sel = state.selection.primary();
    if !extend && !sel.collapsed() {
        return moved(state, Selection::single(sel.normalized().1), now);
    }
    let next = next_grapheme_boundary(value, sel.head);
    let new_sel = if extend { Selection::range(sel.anchor, next) } else { Selection::single(next) };
    moved(state, new_sel, now)
}

/// Move to the start of the previous word (Alt/Ctrl+Left).
pub fn move_word_left(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let sel = state.selection.primary();
    let prev = prev_word_boundary(value, sel.head);
    let new_sel = if extend { Selection::range(sel.anchor, prev) } else { Selection::single(prev) };
    moved(state, new_sel, now)
}

/// Move to the end of the next word (Alt/Ctrl+Right).
pub fn move_word_right(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let sel = state.selection.primary();
    let next = next_word_boundary(value, sel.head);
    let new_sel = if extend { Selection::range(sel.anchor, next) } else { Selection::single(next) };
    moved(state, new_sel, now)
}

pub fn move_home(state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let sel = state.selection.primary();
    let new_sel = if extend { Selection::range(sel.anchor, 0) } else { Selection::single(0) };
    moved(state, new_sel, now)
}

pub fn move_end(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let n = char_count(value);
    let sel = state.selection.primary();
    let new_sel = if extend { Selection::range(sel.anchor, n) } else { Selection::single(n) };
    moved(state, new_sel, now)
}

pub fn select_all(value: &str, state: &TextEditState, now: f32) -> TextEditState {
    moved(state, Selection::range(0, char_count(value)), now)
}

/// The currently selected substring, or `None` when there is no
/// selection.
pub fn selected_text(value: &str, state: &TextEditState) -> Option<String> {
    state.selection_range().map(|(s, e)| {
        let bs = char_byte_offset(value, s);
        let be = char_byte_offset(value, e);
        value[bs..be].to_string()
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Undo / redo
// ─────────────────────────────────────────────────────────────────────────

/// Undo the most recent edit. `None` when the undo stack is empty — a
/// real no-op the caller should skip committing/repainting for.
pub fn undo(value: &str, state: &TextEditState, now: f32) -> Option<(String, TextEditState)> {
    let mut undo_stack = state.undo_stack.clone();
    let entry = undo_stack.pop()?;
    let (new_value, redo_inverse) = entry.inverse.apply(value);
    let mut redo_stack = state.redo_stack.clone();
    redo_stack.push(UndoEntry { inverse: redo_inverse, selection_before: state.selection.clone() });
    Some((
        new_value,
        TextEditState { selection: entry.selection_before, last_edit_at: now, undo_stack, redo_stack, coalesce: None },
    ))
}

/// Redo the most recently undone edit. `None` when the redo stack is
/// empty (or was cleared by an intervening real edit — standard).
pub fn redo(value: &str, state: &TextEditState, now: f32) -> Option<(String, TextEditState)> {
    let mut redo_stack = state.redo_stack.clone();
    let entry = redo_stack.pop()?;
    let (new_value, undo_inverse) = entry.inverse.apply(value);
    let mut undo_stack = state.undo_stack.clone();
    undo_stack.push(UndoEntry { inverse: undo_inverse, selection_before: state.selection.clone() });
    Some((
        new_value,
        TextEditState { selection: entry.selection_before, last_edit_at: now, undo_stack, redo_stack, coalesce: None },
    ))
}

// ─────────────────────────────────────────────────────────────────────────
// Command — the abstract vocabulary a keymap (rosace/src/engine.rs, which
// alone sees rosace_platform::Key) translates key events into (D116 layer
// 4). Character insertion isn't a Command — Text events feed insert_char
// directly (see engine.rs's dispatch comment on why, carried from Step 1).
// ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    MoveLeft, MoveRight, MoveWordLeft, MoveWordRight, MoveHome, MoveEnd,
    ExtendLeft, ExtendRight, ExtendWordLeft, ExtendWordRight, ExtendHome, ExtendEnd,
    Backspace, DeleteForward, DeleteWordBack, DeleteWordForward,
    SelectAll, Copy, Cut, Paste, Undo, Redo,
}

/// Execute a non-clipboard [`Command`]. `Copy`/`Cut`/`Paste` return `None`
/// — clipboard I/O is the caller's job (`rosace/src/engine.rs`, which
/// already owns `rosace-clipboard`; this crate doesn't depend on it).
/// `Undo`/`Redo` also return `None` on a genuinely empty stack; every
/// other command always returns `Some` (even a saturated move bumps
/// `last_edit_at` for the blink reset, matching Step 1).
pub fn apply_command(value: &str, state: &TextEditState, cmd: Command, now: f32) -> Option<(String, TextEditState)> {
    use Command::*;
    Some(match cmd {
        MoveLeft => (value.to_string(), move_left(value, state, false, now)),
        ExtendLeft => (value.to_string(), move_left(value, state, true, now)),
        MoveRight => (value.to_string(), move_right(value, state, false, now)),
        ExtendRight => (value.to_string(), move_right(value, state, true, now)),
        MoveWordLeft => (value.to_string(), move_word_left(value, state, false, now)),
        ExtendWordLeft => (value.to_string(), move_word_left(value, state, true, now)),
        MoveWordRight => (value.to_string(), move_word_right(value, state, false, now)),
        ExtendWordRight => (value.to_string(), move_word_right(value, state, true, now)),
        MoveHome => (value.to_string(), move_home(state, false, now)),
        ExtendHome => (value.to_string(), move_home(state, true, now)),
        MoveEnd => (value.to_string(), move_end(value, state, false, now)),
        ExtendEnd => (value.to_string(), move_end(value, state, true, now)),
        Backspace => backspace(value, state, now),
        DeleteForward => delete_forward(value, state, now),
        DeleteWordBack => delete_word_back(value, state, now),
        DeleteWordForward => delete_word_forward(value, state, now),
        SelectAll => (value.to_string(), select_all(value, state, now)),
        Undo => return undo(value, state, now),
        Redo => return redo(value, state, now),
        Copy | Cut | Paste => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// EditController — the app-facing programmatic handle (D116 layer 5 /
// D101 FocusNode precedent). Unlike `scroll_controller()`'s
// auto-created-per-node shape, an app driving a markdown toolbar's Bold
// button needs a handle reachable from OUTSIDE the widget tree entirely
// (a button's `on_press` has no access to the field's render-tree node) —
// so this is APP-CONSTRUCTED and PASSED IN via `.controller(EditController)`,
// mirroring `FocusNode`. Calls enqueue an op onto a `Send + Sync` channel
// (the render tree itself is `Rc<RefCell<_>>`, unreachable from arbitrary
// closures — the exact constraint `EditableDecl` already documents) and
// wake the frame loop; the engine drains the queue against the matching
// node (found via `id()`, mirroring `FocusManager::focus_owner`) once per
// frame.
// ─────────────────────────────────────────────────────────────────────────

static CONTROLLER_ID: AtomicU64 = AtomicU64::new(1);

/// One pending operation enqueued by an [`EditController`] call — drained
/// and applied by the engine each frame.
#[derive(Clone, Debug)]
pub enum ControllerOp {
    ReplaceRange(usize, usize, String),
    InsertAtCursor(String),
    SetSelection(Selection),
    SelectAll,
    Undo,
    Redo,
}

struct ControllerInner {
    id: u64,
    ops: Mutex<Vec<ControllerOp>>,
    snapshot: Mutex<(String, Selection)>,
}

#[derive(Clone)]
pub struct EditController(Arc<ControllerInner>);

impl EditController {
    pub fn new() -> Self {
        Self(Arc::new(ControllerInner {
            id: CONTROLLER_ID.fetch_add(1, Ordering::Relaxed),
            ops: Mutex::new(Vec::new()),
            snapshot: Mutex::new((String::new(), Selection::default())),
        }))
    }

    /// Unique id — how the engine finds the render-tree node that
    /// declared this controller (same shape as `FocusNode::id`).
    pub fn id(&self) -> u64 {
        self.0.id
    }

    fn enqueue(&self, op: ControllerOp) {
        self.0.ops.lock().unwrap_or_else(|e| e.into_inner()).push(op);
        rosace_state::request_frame();
    }

    /// Replace an explicit `[start, end)` char range — independent of
    /// wherever the caret currently is (the markdown-toolbar-Bold-button
    /// primitive: `replace_range(sel.0, sel.1, format!("**{}**", text))`).
    pub fn replace_range(&self, start: usize, end: usize, text: impl Into<String>) {
        self.enqueue(ControllerOp::ReplaceRange(start, end, text.into()));
    }
    pub fn insert_at_cursor(&self, text: impl Into<String>) {
        self.enqueue(ControllerOp::InsertAtCursor(text.into()));
    }
    pub fn set_selection(&self, sel: Selection) {
        self.enqueue(ControllerOp::SetSelection(sel));
    }
    pub fn select_all(&self) {
        self.enqueue(ControllerOp::SelectAll);
    }
    pub fn undo(&self) {
        self.enqueue(ControllerOp::Undo);
    }
    pub fn redo(&self) {
        self.enqueue(ControllerOp::Redo);
    }

    /// The field's value as of the last time the engine applied a drained
    /// op (a read-only snapshot — not live within the same frame an op
    /// was JUST enqueued; the engine hasn't run yet).
    pub fn value(&self) -> String {
        self.0.snapshot.lock().unwrap_or_else(|e| e.into_inner()).0.clone()
    }
    pub fn selection(&self) -> Selection {
        self.0.snapshot.lock().unwrap_or_else(|e| e.into_inner()).1.clone()
    }

    /// Engine-internal: drain pending ops for this frame. Not part of the
    /// app-facing API (hidden from docs, but must be `pub` for the
    /// `rosace` crate to call it).
    #[doc(hidden)]
    pub fn take_ops(&self) -> Vec<ControllerOp> {
        std::mem::take(&mut *self.0.ops.lock().unwrap_or_else(|e| e.into_inner()))
    }
    /// Engine-internal: publish the post-drain snapshot.
    #[doc(hidden)]
    pub fn update_snapshot(&self, value: String, selection: Selection) {
        *self.0.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = (value, selection);
    }
}

impl Default for EditController {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EditController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EditController(id={})", self.0.id)
    }
}

impl PartialEq for EditController {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(cursor: usize) -> TextEditState {
        TextEditState { selection: Selection::single(cursor), ..Default::default() }
    }
    fn st_sel(anchor: usize, head: usize) -> TextEditState {
        TextEditState { selection: Selection::range(anchor, head), ..Default::default() }
    }

    // ── Step 1 behavior, re-verified against the new internals ─────────

    #[test]
    fn insert_char_at_end() {
        let s = st(5);
        let (v, ns) = insert_char("hello", &s, '!', 1.0);
        assert_eq!(v, "hello!");
        assert_eq!(ns.cursor(), 6);
        assert_eq!(ns.last_edit_at, 1.0);
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn insert_char_in_middle() {
        let (v, ns) = insert_char("helo", &st(3), 'l', 0.0);
        assert_eq!(v, "hello");
        assert_eq!(ns.cursor(), 4);
    }

    #[test]
    fn insert_str_replaces_selection() {
        let s = st_sel(6, 11); // "hello world", select "world"
        let (v, ns) = insert_str("hello world", &s, "there", 2.0);
        assert_eq!(v, "hello there");
        assert_eq!(ns.cursor(), 11);
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn insert_handles_multibyte_utf8_without_panicking() {
        let (v, ns) = insert_char("café", &st(4), '!', 0.0);
        assert_eq!(v, "café!");
        assert_eq!(ns.cursor(), 5);
    }

    #[test]
    fn backspace_removes_char_before_cursor() {
        let (v, ns) = backspace("hello", &st(5), 1.0);
        assert_eq!(v, "hell");
        assert_eq!(ns.cursor(), 4);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let (v, ns) = backspace("hello", &st(0), 1.0);
        assert_eq!(v, "hello");
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn backspace_deletes_selection_instead_of_one_char() {
        let s = st_sel(1, 5);
        let (v, ns) = backspace("hello", &s, 1.0);
        assert_eq!(v, "h");
        assert_eq!(ns.cursor(), 1);
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn delete_forward_removes_char_after_cursor() {
        let (v, ns) = delete_forward("hello", &st(0), 1.0);
        assert_eq!(v, "ello");
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let (v, ns) = delete_forward("hello", &st(5), 1.0);
        assert_eq!(v, "hello");
    }

    #[test]
    fn move_left_decrements_and_clears_selection() {
        let ns = move_left("hello", &st(3), false, 1.0);
        assert_eq!(ns.cursor(), 2);
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn move_left_saturates_at_zero() {
        let ns = move_left("hello", &st(0), false, 1.0);
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn move_left_without_extend_collapses_selection_to_start() {
        let s = st_sel(1, 5);
        let ns = move_left("hello", &s, false, 1.0);
        assert_eq!(ns.cursor(), 1, "must jump to selection start, not head-1");
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn move_right_without_extend_collapses_selection_to_end() {
        let s = st_sel(5, 1);
        let ns = move_right("hello", &s, false, 1.0);
        assert_eq!(ns.cursor(), 5);
        assert!(ns.selection_range().is_none());
    }

    #[test]
    fn move_right_extends_selection_from_fresh_anchor() {
        let ns = move_right("hello", &st(2), true, 1.0);
        assert_eq!(ns.cursor(), 3);
        assert_eq!(ns.selection.primary().anchor, 2, "anchor seeds at the pre-move cursor");
    }

    #[test]
    fn move_right_saturates_at_length() {
        let ns = move_right("hi", &st(2), false, 1.0);
        assert_eq!(ns.cursor(), 2);
    }

    #[test]
    fn shift_arrow_sequence_grows_then_shrinks_selection() {
        let s0 = st(2);
        let s1 = move_right("hello world", &s0, true, 0.0);
        let s2 = move_right("hello world", &s1, true, 0.0);
        assert_eq!(s2.selection_range(), Some((2, 4)));
        let s3 = move_left("hello world", &s2, true, 0.0);
        assert_eq!(s3.selection_range(), Some((2, 3)));
    }

    #[test]
    fn move_home_and_end() {
        let h = move_home(&st(3), false, 1.0);
        assert_eq!(h.cursor(), 0);
        let e = move_end("hello", &st(0), false, 1.0);
        assert_eq!(e.cursor(), 5);
    }

    #[test]
    fn select_all_selects_full_range() {
        let s = select_all("hello", &st(0), 1.0);
        assert_eq!(s.cursor(), 5);
        assert_eq!(s.selection_range(), Some((0, 5)));
    }

    #[test]
    fn selected_text_extracts_the_right_substring() {
        let s = st_sel(6, 11);
        assert_eq!(selected_text("hello world", &s).as_deref(), Some("world"));
    }

    #[test]
    fn selected_text_none_when_anchor_equals_cursor() {
        let s = st(3);
        assert_eq!(selected_text("hello", &s), None);
        assert_eq!(s.selection_range(), None);
    }

    #[test]
    fn selection_range_normalizes_backward_selection() {
        let s = st_sel(5, 2); // Shift+Left from 5 to 2
        assert_eq!(s.selection_range(), Some((2, 5)));
    }

    // ── Step 2: transactions / undo / redo / coalescing ─────────────────

    #[test]
    fn transaction_apply_and_invert_round_trips() {
        let txn = Transaction::single((2, 2), "XY");
        let (v1, inv) = txn.apply("hello");
        assert_eq!(v1, "heXYllo");
        let (v2, _) = inv.apply(&v1);
        assert_eq!(v2, "hello", "applying the inverse must reconstruct the original exactly");
    }

    #[test]
    fn undo_reverts_an_insertion_and_restores_prior_selection() {
        let s0 = st(0);
        let (v1, s1) = insert_str("", &s0, "hi", 1.0);
        assert_eq!(v1, "hi");
        assert!(s1.can_undo());

        let (v2, s2) = undo(&v1, &s1, 2.0).expect("undo must produce a result");
        assert_eq!(v2, "");
        assert_eq!(s2.cursor(), 0, "must restore the pre-edit selection");
        assert!(!s2.can_undo());
        assert!(s2.can_redo());
    }

    #[test]
    fn redo_reapplies_an_undone_edit() {
        let s0 = st(0);
        let (v1, s1) = insert_str("", &s0, "hi", 1.0);
        let (v2, s2) = undo(&v1, &s1, 2.0).unwrap();
        let (v3, s3) = redo(&v2, &s2, 3.0).expect("redo must produce a result");
        assert_eq!(v3, "hi");
        assert_eq!(s3.cursor(), 2);
        assert!(s3.can_undo());
        assert!(!s3.can_redo());
    }

    #[test]
    fn undo_on_empty_stack_is_none() {
        let s0 = st(0);
        assert!(undo("hello", &s0, 1.0).is_none());
    }

    #[test]
    fn a_real_edit_after_undo_clears_the_redo_stack() {
        let s0 = st(0);
        let (v1, s1) = insert_str("", &s0, "a", 1.0);
        let (v2, s2) = undo(&v1, &s1, 2.0).unwrap();
        assert!(s2.can_redo());
        let (_, s3) = insert_str(&v2, &s2, "b", 3.0);
        assert!(!s3.can_redo(), "a fresh edit must invalidate the old redo branch");
    }

    #[test]
    fn consecutive_typing_coalesces_into_one_undo_unit() {
        let s0 = st(0);
        let (v1, s1) = insert_char("", &s0, 'a', 1.0);
        let (v2, s2) = insert_char(&v1, &s1, 'b', 1.1);
        let (v3, s3) = insert_char(&v2, &s2, 'c', 1.2);
        assert_eq!(v3, "abc");

        let (v4, s4) = undo(&v3, &s3, 2.0).expect("one undo");
        assert_eq!(v4, "", "one undo must remove the WHOLE typed group");
        assert_eq!(s4.cursor(), 0, "must restore the selection from BEFORE the whole group");
        assert!(!s4.can_undo(), "the group must have been a single undo entry");
    }

    #[test]
    fn typing_separated_by_a_pause_does_not_coalesce() {
        let s0 = st(0);
        let (v1, s1) = insert_char("", &s0, 'a', 0.0);
        // Past the coalesce window.
        let (v2, s2) = insert_char(&v1, &s1, 'b', 0.0 + COALESCE_WINDOW_SECS + 0.01);
        assert_eq!(v2, "ab");
        let (v3, s3) = undo(&v2, &s2, 1.0).unwrap();
        assert_eq!(v3, "a", "only the second, un-coalesced char should undo");
        assert!(s3.can_undo(), "the first char's group must still be on the stack");
    }

    #[test]
    fn typing_after_a_cursor_move_does_not_coalesce_with_earlier_typing() {
        let s0 = st(0);
        let (v1, s1) = insert_char("", &s0, 'a', 1.0);
        let s1_moved = move_left("a", &s1, false, 1.05); // move breaks the group
        let (v2, s2) = insert_char(&v1, &s1_moved, 'b', 1.06);
        assert_eq!(v2, "ba");
        let (v3, s3) = undo(&v2, &s2, 2.0).unwrap();
        assert_eq!(v3, "a", "only the second char's group should undo");
        assert!(s3.can_undo());
    }

    #[test]
    fn replacing_a_selection_does_not_coalesce_with_prior_typing() {
        let s0 = st(0);
        let (v1, s1) = insert_str("", &s0, "hello", 1.0);
        let s1_sel = TextEditState { selection: Selection::range(1, 3), ..s1.clone() };
        let (v2, s2) = insert_str(&v1, &s1_sel, "X", 1.1);
        assert_eq!(v2, "hXlo");
        let (v3, _) = undo(&v2, &s2, 2.0).unwrap();
        assert_eq!(v3, "hello", "undoing the selection-replace must not also undo the typed word");
    }

    // ── Step 2: grapheme-cluster correctness ─────────────────────────────

    #[test]
    fn backspace_deletes_a_whole_zwj_family_emoji_in_one_press() {
        // Man+Woman+Girl+Boy joined by ZWJ — ONE grapheme, several chars.
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let n = char_count(family);
        let s = st(n);
        let (v, ns) = backspace(family, &s, 1.0);
        assert_eq!(v, "", "the whole cluster must vanish in one Backspace, not one char at a time");
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn move_left_steps_over_a_combining_accent_as_one_unit() {
        // 'e' + COMBINING ACUTE ACCENT (U+0301) — two chars, one grapheme.
        let s = "e\u{0301}x"; // "é" (decomposed) + "x"
        assert_eq!(char_count(s), 3);
        let state = st(3); // cursor after "x"
        let after_one_left = move_left(s, &state, false, 1.0);
        assert_eq!(after_one_left.cursor(), 2, "must land after the é-cluster, before x");
        let after_two_left = move_left(s, &after_one_left, false, 1.0);
        assert_eq!(after_two_left.cursor(), 0, "the combining accent must not be a stop of its own");
    }

    #[test]
    fn delete_forward_removes_a_flag_emoji_as_one_grapheme() {
        // Regional indicator pair — two chars (surrogate-pair-free BMP+1
        // codepoints), one grapheme (a flag).
        let flag = "🇮🇳x";
        let s = st(0);
        let (v, ns) = delete_forward(flag, &s, 1.0);
        assert_eq!(v, "x", "the flag must vanish as one unit, not one regional indicator at a time");
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn plain_ascii_grapheme_boundaries_match_char_boundaries() {
        // Sanity: for plain text every grapheme boundary is a char
        // boundary — the Step 1 behavior above is a special case of this.
        assert_eq!(grapheme_boundaries("abc"), vec![0, 1, 2, 3]);
    }

    // ── Step 2: word-wise movement/deletion ──────────────────────────────

    #[test]
    fn move_word_right_lands_at_the_end_of_the_next_word() {
        let s = st(0);
        let ns = move_word_right("hello world", &s, false, 1.0);
        assert_eq!(ns.cursor(), 5);
        let ns2 = move_word_right("hello world", &ns, false, 1.0);
        assert_eq!(ns2.cursor(), 11);
    }

    #[test]
    fn move_word_left_lands_at_the_start_of_the_previous_word() {
        let s = st(11); // end of "hello world"
        let ns = move_word_left("hello world", &s, false, 1.0);
        assert_eq!(ns.cursor(), 6);
        let ns2 = move_word_left("hello world", &ns, false, 1.0);
        assert_eq!(ns2.cursor(), 0);
    }

    #[test]
    fn delete_word_back_removes_the_preceding_word() {
        let s = st(11); // "hello world", cursor at end
        let (v, ns) = delete_word_back("hello world", &s, 1.0);
        assert_eq!(v, "hello ");
        assert_eq!(ns.cursor(), 6);
    }

    #[test]
    fn delete_word_forward_removes_the_following_word() {
        let s = st(0);
        let (v, ns) = delete_word_forward("hello world", &s, 1.0);
        assert_eq!(v, " world");
        assert_eq!(ns.cursor(), 0);
    }

    #[test]
    fn extend_word_right_selects_through_a_word() {
        let s = st(0);
        let ns = move_word_right("hello world", &s, true, 1.0);
        assert_eq!(ns.selection_range(), Some((0, 5)));
    }

    // ── Step 2: Command dispatch ──────────────────────────────────────

    #[test]
    fn apply_command_backspace_matches_the_direct_call() {
        let s = st(5);
        let (v1, s1) = apply_command("hello", &s, Command::Backspace, 1.0).unwrap();
        let (v2, s2) = backspace("hello", &s, 1.0);
        assert_eq!(v1, v2);
        assert_eq!(s1.cursor(), s2.cursor());
    }

    #[test]
    fn apply_command_clipboard_commands_return_none() {
        let s = st(0);
        assert!(apply_command("hello", &s, Command::Copy, 1.0).is_none());
        assert!(apply_command("hello", &s, Command::Cut, 1.0).is_none());
        assert!(apply_command("hello", &s, Command::Paste, 1.0).is_none());
    }

    #[test]
    fn apply_command_undo_on_empty_history_returns_none() {
        let s = st(0);
        assert!(apply_command("hello", &s, Command::Undo, 1.0).is_none());
    }

    // ── Step 2: EditController (the toolbar Bold-button scenario) ───────

    #[test]
    fn edit_controller_replace_range_wraps_a_selection_like_a_toolbar_button() {
        let value = "hello world";
        let state = st_sel(6, 11); // "world" selected

        let controller = EditController::new();
        assert!(controller.take_ops().is_empty());

        // The exact toolbar-Bold-button shape D116 promises: read the
        // selection, wrap it, replace_range.
        let (start, end) = state.selection_range().unwrap();
        controller.replace_range(start, end, format!("**{}**", &value[start..end]));

        let ops = controller.take_ops();
        assert_eq!(ops.len(), 1);
        let ControllerOp::ReplaceRange(s, e, text) = &ops[0] else { panic!("expected ReplaceRange") };
        assert_eq!((*s, *e, text.as_str()), (6, 11, "**world**"));

        let (new_value, new_state) = replace_range(value, &state, *s, *e, text, 1.0);
        assert_eq!(new_value, "hello **world**");
        assert_eq!(new_state.cursor(), 15);

        controller.update_snapshot(new_value.clone(), new_state.selection.clone());
        assert_eq!(controller.value(), "hello **world**");
    }

    #[test]
    fn edit_controller_has_a_stable_id_distinct_from_other_controllers() {
        let a = EditController::new();
        let b = EditController::new();
        assert_ne!(a.id(), b.id());
        assert_eq!(a.clone().id(), a.id(), "cloning must share identity, not create a new controller");
    }

    #[test]
    fn edit_controller_undo_redo_ops_enqueue_correctly() {
        let c = EditController::new();
        c.undo();
        c.redo();
        c.select_all();
        let ops = c.take_ops();
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0], ControllerOp::Undo));
        assert!(matches!(ops[1], ControllerOp::Redo));
        assert!(matches!(ops[2], ControllerOp::SelectAll));
    }
}
