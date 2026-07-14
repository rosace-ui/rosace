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
use rosace_render::{Color, FontWeight};
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
        edits.sort_by_key(|e| std::cmp::Reverse(e.range.0)); // highest start first

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

/// The union char range (in the NEW value's coordinate space, after
/// `edits` has been applied) touched by a set of edits — the
/// `TextEditState::last_edit_range` this transaction produces (D116 Step
/// 5). A conservative min-start/max-end union across multiple edits
/// (multi-cursor's future shape) is still far smaller than "the whole
/// document" for any realistic edit.
fn edits_affected_range(edits: &[Edit]) -> Option<(usize, usize)> {
    edits.iter().map(|e| (e.range.0, e.range.0 + char_count(&e.replacement)))
        .fold(None, |acc: Option<(usize, usize)>, r| Some(match acc {
            None => r,
            Some(a) => (a.0.min(r.0), a.1.max(r.1)),
        }))
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

/// The `[start, end)` word run containing `pos` — double-click-to-select
/// (D116 Step 3). A click inside whitespace/punctuation selects that
/// whitespace run itself (matches most editors: double-clicking a gap
/// selects the gap, not the nearest word).
pub fn word_range_at(s: &str, pos: usize) -> (usize, usize) {
    let runs = word_bound_boundaries(s);
    let n = char_count(s);
    let mut i = 0;
    while i + 1 < runs.len() && runs[i + 1].0 <= pos {
        i += 1;
    }
    let start = runs[i].0;
    let end = runs.get(i + 1).map(|&(st, _)| st).unwrap_or(n);
    (start, end)
}

// ─────────────────────────────────────────────────────────────────────────
// TextLayoutSnapshot (D116 layer 3) — the keystone seam. Built during
// paint (where FontCache IS available) as plain, Send+Sync-free data;
// engine dispatch queries it with ZERO font access, dissolving the
// `!Sync` wall Step 1's EditableDecl doc comment names. One structure
// answers every pointer-positioning question: click-to-glyph, drag
// selection, double/triple-click, and (Step 6) the IME candidate-window
// rect.
// ─────────────────────────────────────────────────────────────────────────

/// One line's geometry within a [`TextLayoutSnapshot`]. `TextInput`
/// (single-line) always has exactly one; `TextArea` (Step 4) will have
/// one per wrapped visual line — the shape already supports that, so
/// Step 4 needs no snapshot redesign.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineLayout {
    /// Char range `[start, end)` this line's text spans (end exclusive
    /// of any line-break character).
    pub char_range: (usize, usize),
    /// Top y and height, WORLD-SPACE (matching `EditableDecl::rect`) —
    /// so engine dispatch can query directly against raw click
    /// coordinates with no translation.
    pub y: f32,
    pub height: f32,
    /// Absolute char index of every grapheme boundary in this line,
    /// ascending, paired 1:1 with `boundary_x`.
    pub boundary_chars: Vec<usize>,
    /// World-space x of each boundary in `boundary_chars`.
    pub boundary_x: Vec<f32>,
}

impl LineLayout {
    /// World-space x of `target` within this line — clamped to the
    /// line's own `char_range` first, so an index from a multi-line
    /// selection or a `Span` that extends past this line's end/start
    /// still resolves to a sane on-screen position (this line's own
    /// right/left edge) instead of panicking or guessing.
    pub fn x_at(&self, target: usize) -> f32 {
        let clamped = target.clamp(self.char_range.0, self.char_range.1);
        if let Some(i) = self.boundary_chars.iter().position(|&c| c == clamped) {
            self.boundary_x[i]
        } else if clamped <= self.char_range.0 {
            self.boundary_x.first().copied().unwrap_or(0.0)
        } else {
            self.boundary_x.last().copied().unwrap_or(0.0)
        }
    }
}

/// Plain-data paint-time geometry for an editable field (D116 layer 3).
/// Declared fresh each paint on [`EditableDecl`], like everything else
/// there — never mutated by dispatch, only read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayoutSnapshot {
    pub lines: Vec<LineLayout>,
}

impl TextLayoutSnapshot {
    fn line_for_y(&self, y: f32) -> Option<&LineLayout> {
        if self.lines.is_empty() {
            return None;
        }
        for line in &self.lines {
            if y < line.y + line.height {
                return Some(line);
            }
        }
        self.lines.last()
    }

    /// The grapheme boundary nearest `(x, y)` — the click-to-caret
    /// primitive. Picks the line whose y-band contains (or is nearest)
    /// `y`, then within that line the boundary straddling `x`, snapped
    /// to whichever side of the nearest glyph's midpoint `x` falls on
    /// (the universal "click past halfway selects the position after
    /// it" convention). Returns 0 if the snapshot has no lines at all
    /// (shouldn't happen for a real editable widget's own paint).
    pub fn position_at(&self, x: f32, y: f32) -> usize {
        let Some(line) = self.line_for_y(y) else { return 0; };
        if line.boundary_x.is_empty() {
            return line.char_range.0;
        }
        let mut idx = 0usize;
        for (i, &bx) in line.boundary_x.iter().enumerate() {
            if bx <= x {
                idx = i;
            } else {
                break;
            }
        }
        if idx + 1 < line.boundary_x.len() {
            let mid = (line.boundary_x[idx] + line.boundary_x[idx + 1]) / 2.0;
            if x > mid {
                idx += 1;
            }
        }
        line.boundary_chars[idx]
    }

    /// World-space x of `char_idx`, if it's a known boundary in some
    /// line — caret rendering and scroll-into-view both want this
    /// (computed once here rather than re-measuring text at paint time
    /// AND again at dispatch time from two different code paths).
    pub fn x_of(&self, char_idx: usize) -> Option<f32> {
        for line in &self.lines {
            if let Some(i) = line.boundary_chars.iter().position(|&c| c == char_idx) {
                return Some(line.boundary_x[i]);
            }
        }
        None
    }

    /// The `[start, end)` range of the line containing `char_idx` —
    /// triple-click-to-select-line. For `TextInput` (one line spanning
    /// the whole value) this is equivalent to select-all; `TextArea`
    /// (Step 4) gets real per-visual-line selection for free.
    pub fn line_range_at(&self, char_idx: usize) -> (usize, usize) {
        for line in &self.lines {
            if char_idx >= line.char_range.0 && char_idx <= line.char_range.1 {
                return line.char_range;
            }
        }
        self.lines.last().map(|l| l.char_range).unwrap_or((0, 0))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Span + style_runs (D116 layer 5) — THE markdown/syntax-highlighting
// seam. The core never learns what markdown, JSON, or a language grammar
// is; the app supplies a `SpanSource` closure (`TextInput::spans`/
// `TextArea::spans`) that inspects the current value and returns colored/
// weighted ranges. `style_runs` is the shared primitive both widgets use
// to turn an arbitrary (possibly overlapping, possibly gappy) span list
// into a contiguous, non-overlapping paint plan for one line.
// ─────────────────────────────────────────────────────────────────────────

/// One styled range of the document — a token from the app's own
/// tokenizer (a markdown bold run, a JSON string, a syntax-highlighted
/// keyword). `None` fields fall back to the widget's own default text
/// color/weight, so a `SpanSource` only needs to describe what it wants
/// to OVERRIDE, not restate the whole style for every char.
#[derive(Clone, Debug)]
pub struct Span {
    pub range: (usize, usize),
    pub color: Option<Color>,
    pub weight: Option<FontWeight>,
}

impl PartialEq for Span {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range
            && self.color.map(color_bits) == other.color.map(color_bits)
            && self.weight == other.weight
    }
}

/// `Color` has no `PartialEq` of its own (not every `rosace-render`
/// consumer needs one) — this is the local, comparable projection `Span`/
/// `CursorStyle` equality needs.
fn color_bits(c: Color) -> (u8, u8, u8, u8) { (c.r, c.g, c.b, c.a) }

impl Span {
    pub fn new(range: (usize, usize)) -> Self {
        Self { range, color: None, weight: None }
    }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn weight(mut self, w: FontWeight) -> Self { self.weight = Some(w); self }
}

/// The app-supplied tokenizer hook (`TextInput::spans`/`TextArea::spans`).
/// Called with the CURRENT value and, when available, the char range that
/// changed since the last call (`None` on the very first call, or when
/// the whole document should be considered changed — e.g. a controller-
/// driven `replace_range` spanning most of the text) — an incremental
/// tokenizer uses this to only re-scan the affected region instead of the
/// whole document every keystroke.
pub type SpanFn = dyn Fn(&str, Option<(usize, usize)>) -> Vec<Span> + Send + Sync;

/// Split `[ls, le)` into contiguous, non-overlapping style runs from
/// `spans` — gaps between/around spans become `(range, None, None)` runs
/// (the widget's own default color/weight). When multiple spans cover the
/// same sub-range, the LAST one in `spans` wins (simple override
/// semantics — good enough for "syntax highlighting layered over a base
/// style", the common case; spans are not expected to be a stacking z-order).
pub fn style_runs(spans: &[Span], ls: usize, le: usize) -> Vec<(usize, usize, Option<Color>, Option<FontWeight>)> {
    if ls >= le {
        return Vec::new();
    }
    let mut points: Vec<usize> = vec![ls, le];
    for s in spans {
        if s.range.0 > ls && s.range.0 < le { points.push(s.range.0); }
        if s.range.1 > ls && s.range.1 < le { points.push(s.range.1); }
    }
    points.sort_unstable();
    points.dedup();
    points.windows(2).map(|w| {
        let (a, b) = (w[0], w[1]);
        let cover = spans.iter().rev().find(|s| s.range.0 <= a && s.range.1 >= b);
        (a, b, cover.and_then(|s| s.color), cover.and_then(|s| s.weight))
    }).collect()
}

// ─────────────────────────────────────────────────────────────────────────
// CursorStyle (D116 layer 5) — we already paint the caret ourselves
// (never the OS's), so it's fully themable: width, color, corner radius,
// blink rate, and shape, including a `Custom` app-supplied painter (an
// icon, a shader fill, anything `PaintCtx` can record). Theme-level
// default via `ThemeData::ext`/`with_ext` (D105 Phase 23's type-keyed
// extension map — no edit to `ThemeData` itself needed); per-field
// override via `.cursor_style()` wins when set.
// ─────────────────────────────────────────────────────────────────────────

/// An app-supplied caret painter: `Fn(&mut PaintCtx, caret_rect)` —
/// see [`CursorShape::Custom`].
pub type CursorPainter = Arc<dyn Fn(&mut super::PaintCtx, Rect) + Send + Sync>;

/// How the caret renders. `Custom`'s painter receives the caret's
/// world-space rect (position + the field's own line height) and paints
/// whatever it wants — the default shapes below are themselves just
/// convenience presets a `Custom` painter could fully replicate.
#[derive(Clone)]
pub enum CursorShape {
    /// A thin vertical bar at the caret position — the universal default.
    Bar,
    /// A filled block spanning to the next glyph boundary (or a fallback
    /// width at end-of-line) — the classic terminal/overwrite-mode caret.
    Block,
    /// A thin bar at the BOTTOM of the line instead of a vertical stroke.
    Underline,
    /// App-supplied painter: `Fn(&mut PaintCtx, caret_rect)`.
    Custom(CursorPainter),
}

impl std::fmt::Debug for CursorShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorShape::Bar => write!(f, "Bar"),
            CursorShape::Block => write!(f, "Block"),
            CursorShape::Underline => write!(f, "Underline"),
            CursorShape::Custom(_) => write!(f, "Custom(..)"),
        }
    }
}

impl PartialEq for CursorShape {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (CursorShape::Bar, CursorShape::Bar)
                | (CursorShape::Block, CursorShape::Block)
                | (CursorShape::Underline, CursorShape::Underline)
        ) // Custom painters are never equal, even to themselves — not a meaningful comparison.
    }
}

#[derive(Clone, Debug)]
pub struct CursorStyle {
    pub width: f32,
    pub color: Color,
    pub corner_radius: f32,
    /// Seconds per half-cycle (on/off) of the blink — matches the
    /// pre-Step-5 hardcoded `0.53` by default.
    pub blink_rate: f32,
    pub shape: CursorShape,
}

impl PartialEq for CursorStyle {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && color_bits(self.color) == color_bits(other.color)
            && self.corner_radius == other.corner_radius
            && self.blink_rate == other.blink_rate
            && self.shape == other.shape
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            width: 1.5,
            color: Color::rgb(180, 160, 255),
            corner_radius: 0.0,
            blink_rate: 0.53,
            shape: CursorShape::Bar,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// InputFilter (D116 Step 8) — applied at the ONE funnel every edit
// source reaches (`engine.rs`'s `commit_text_edit`), not per transaction-
// builder function, so typed chars/paste/IME-commit/controller ops all
// get filtered identically without duplicating the check everywhere.
// Deliberately separate from `rosace_forms::Validator`: a filter REJECTS
// characters as you type ("this field can never contain a comma"); a
// validator judges a COMPLETE value ("this field must be a valid
// email") — conflating them would make simple things (max length) need
// a whole `FormField` to express.
// ─────────────────────────────────────────────────────────────────────────

/// One input filter — see the module-section doc above for why this is
/// a separate concept from `rosace_forms::Validator`.
#[derive(Clone)]
pub enum InputFilter {
    /// Reject any edit that would push the value's CHAR count past `n`
    /// (truncates from the end).
    MaxLength(usize),
    /// Keep only characters for which the predicate returns `true`.
    CharClass(Arc<dyn Fn(char) -> bool + Send + Sync>),
}

impl InputFilter {
    pub fn max_length(n: usize) -> Self { InputFilter::MaxLength(n) }
    pub fn char_class(f: impl Fn(char) -> bool + Send + Sync + 'static) -> Self {
        InputFilter::CharClass(Arc::new(f))
    }
    /// Convenience: digits only (a numeric field).
    pub fn digits() -> Self { Self::char_class(|c| c.is_ascii_digit()) }
    /// Convenience: letters and digits only.
    pub fn alphanumeric() -> Self { Self::char_class(|c| c.is_alphanumeric()) }
}

/// Apply `filters` to `value` in order. Pure function — used by
/// `engine.rs`'s `commit_text_edit`, the single funnel every edit source
/// (typed chars, paste, IME commit, controller ops) reaches.
pub fn apply_filters(value: &str, filters: &[InputFilter]) -> String {
    let mut v = value.to_string();
    for f in filters {
        v = match f {
            InputFilter::CharClass(pred) => v.chars().filter(|&c| pred(c)).collect(),
            InputFilter::MaxLength(n) => v.chars().take(*n).collect(),
        };
    }
    v
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
    /// Paint-time glyph geometry (D116 layer 3) — click-to-glyph, drag
    /// selection, and double/triple-click all query this with zero
    /// `FontCache` access.
    pub layout: TextLayoutSnapshot,
    /// Input filters (D116 Step 8) — see the module section above.
    pub filters: Vec<InputFilter>,
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
    /// Horizontal scroll-into-view offset (D116 Step 3) — how far the
    /// content is shifted left so the caret stays visible when the
    /// value overflows the field's width. Persistent so it doesn't
    /// reset to 0 every repaint; recomputed/clamped by the widget each
    /// paint against the current `TextLayoutSnapshot`.
    pub scroll_x: f32,
    /// Goal-column memory for vertical caret movement across wrapped
    /// lines (D116 Step 4, `TextArea`) — the on-screen x the caret is
    /// "trying" to stay at while Up/Down walks through shorter lines,
    /// same convention every real editor uses. Set on the first vertical
    /// move from the caret's actual x, reused unchanged by consecutive
    /// vertical moves, cleared by any horizontal move/edit/click (see
    /// `moved`/`apply_and_record`) so a fresh vertical move recomputes it.
    pub goal_x: Option<f32>,
    /// The char range (in the NEW value's coordinate space) touched by
    /// the most recent CONTENT edit — `None` after a pure movement/
    /// selection change, or when nothing has been edited yet (D116 Step
    /// 5). This is what makes `SpanSource` incremental: the widget passes
    /// it straight through as the tokenizer's `changed_range` argument
    /// instead of always re-scanning the whole document.
    pub last_edit_range: Option<(usize, usize)>,
    /// Char range currently occupied by an UNCOMMITTED IME preedit
    /// composition (D116 Step 6) — `TextInput`/`TextArea` render an
    /// underline decoration under it (the universal CJK-composition
    /// convention). Set/replaced by `ime_set_preedit`, cleared by
    /// `ime_commit` and by any other movement/edit (composing text is not
    /// something a click or Cmd+Z should have to know about).
    pub ime_range: Option<(usize, usize)>,
    /// The text that was at `ime_range`'s position BEFORE this composition
    /// session started — captured once (the first `ime_set_preedit` call
    /// after `ime_range` was `None`), carried unchanged through later
    /// preedit updates, consumed by `ime_commit` so undoing the WHOLE
    /// composition is one hop back to this, not to the last intermediate
    /// preedit snapshot. Not exposed (`ime_range` is the public rendering
    /// hook; this is bookkeeping).
    ime_origin: Option<String>,
    /// The caret position vertical scroll-into-view has already chased
    /// (`TextArea`) — VIEW state like `scroll_x`, written by the WIDGET
    /// during paint (via `PaintCtx::set_scrolled_cursor`), unlike every
    /// document/selection field above, which only the engine mutates.
    /// The widget chases the caret ONLY when `cursor()` differs from
    /// this, then records the new position. Without the gate the chase
    /// ran every focused frame and FOUGHT wheel input: a caret on a
    /// bottom line snapped every scroll-up straight back (live bug,
    /// 2026-07-12 — "no scrolling when the cursor is at the bottom").
    pub scrolled_cursor: Option<usize>,
}

impl Default for TextEditState {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            last_edit_at: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            coalesce: None,
            scroll_x: 0.0,
            goal_x: None,
            last_edit_range: None,
            ime_range: None,
            ime_origin: None,
            scrolled_cursor: None,
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
    apply_and_record_with_inverse(value, state, txn, None, new_selection, now, coalesce_key)
}

/// Same as [`apply_and_record`], but lets the caller override the
/// recorded undo inverse instead of using `txn`'s own auto-computed one
/// (D116 Step 6 — `ime_commit` needs undo to restore the PRE-composition
/// text, not the last intermediate preedit snapshot `txn.apply`'s normal
/// inverse would compute against the already-preedit-mutated live value).
fn apply_and_record_with_inverse(
    value: &str,
    state: &TextEditState,
    txn: Transaction,
    inverse_override: Option<Transaction>,
    new_selection: Selection,
    now: f32,
    coalesce_key: Option<usize>,
) -> (String, TextEditState) {
    let (new_value, auto_inverse) = txn.apply(value);
    let inverse = inverse_override.unwrap_or(auto_inverse);

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
    let last_edit_range = edits_affected_range(&txn.edits);

    let ns = TextEditState {
        selection: new_selection,
        last_edit_at: now,
        undo_stack,
        redo_stack: Vec::new(),
        coalesce,
        scroll_x: state.scroll_x,
        goal_x: None,
        last_edit_range,
        ime_range: None,
        ime_origin: None,
        scrolled_cursor: state.scrolled_cursor,
    };
    (new_value, ns)
}

// ─────────────────────────────────────────────────────────────────────────
// IME preedit / commit (D116 Step 6) — the "provisional transaction"
// model: preedit text is inserted into the value AS IF typed (so it
// paints, wraps, and click-hit-tests exactly like real content — no
// separate rendering path), but does NOT touch the undo stack. Only
// `ime_commit` records a real (single, whole-composition) undo entry —
// composing "にほん" one romaji keystroke at a time must not produce ten
// undo steps.
// ─────────────────────────────────────────────────────────────────────────

/// Replace the current provisional (uncommitted) range with `text` — a
/// new preedit update from the platform's IME. `cursor_in_text` is the
/// CHAR offset within `text` to place the caret (from the platform's
/// preedit cursor position; `None` places it at the end, matching most
/// IMEs' default). Empty `text` clears the composition (the user deleted
/// through their entire preedit buffer).
pub fn ime_set_preedit(
    value: &str, state: &TextEditState, text: &str, cursor_in_text: Option<usize>, now: f32,
) -> (String, TextEditState) {
    let (start, end) = state.ime_range.unwrap_or_else(|| state.selection.primary_range());
    // Capture the pre-composition text ONCE, the first update of a fresh
    // session (`state.ime_range` was `None`) — carried unchanged through
    // later updates so `ime_commit` can undo the whole thing in one hop.
    let origin = state.ime_origin.clone().unwrap_or_else(|| {
        let sb = char_byte_offset(value, start);
        let eb = char_byte_offset(value, end);
        value[sb..eb].to_string()
    });
    let txn = Transaction::single((start, end), text);
    let (new_value, _auto_inverse) = txn.apply(value);
    let len = char_count(text);
    let new_range = if len == 0 { None } else { Some((start, start + len)) };
    let new_origin = if len == 0 { None } else { Some(origin) };
    let cursor = start + cursor_in_text.unwrap_or(len).min(len);
    let ns = TextEditState {
        selection: Selection::single(cursor),
        last_edit_at: now,
        coalesce: None,
        goal_x: None,
        last_edit_range: Some((start, start + len)),
        ime_range: new_range,
        ime_origin: new_origin,
        ..state.clone()
    };
    (new_value, ns)
}

/// Finalize the composition: replace the provisional range with `text` as
/// a REAL, undoable edit and clear `ime_range`/`ime_origin`. The recorded
/// undo inverse restores `ime_origin` (the PRE-composition text) directly
/// — one Cmd+Z removes the whole committed word, not just the last
/// preedit snapshot (`apply_and_record_with_inverse`'s whole reason for
/// existing). If there's no active composition (a commit with no
/// preceding preedit — some IMEs do this for single-candidate
/// confirmations), replaces the current selection instead, same as a
/// normal insert.
pub fn ime_commit(value: &str, state: &TextEditState, text: &str, now: f32) -> (String, TextEditState) {
    let (start, end) = state.ime_range.unwrap_or_else(|| state.selection.primary_range());
    let origin = state.ime_origin.clone().unwrap_or_else(|| {
        let sb = char_byte_offset(value, start);
        let eb = char_byte_offset(value, end);
        value[sb..eb].to_string()
    });
    let txn = Transaction::single((start, end), text);
    let committed_len = char_count(text);
    let real_inverse = Transaction::single((start, start + committed_len), origin);
    let cursor = start + committed_len;
    let (new_value, ns) = apply_and_record_with_inverse(
        value, state, txn, Some(real_inverse), Selection::single(cursor), now, None,
    );
    (new_value, TextEditState { ime_range: None, ime_origin: None, ..ns })
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
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, goal_x: None, last_edit_range: None, ime_range: None, ..state.clone() });
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
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, goal_x: None, last_edit_range: None, ime_range: None, ..state.clone() });
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
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, goal_x: None, last_edit_range: None, ime_range: None, ..state.clone() });
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
        return (value.to_string(), TextEditState { last_edit_at: now, coalesce: None, goal_x: None, last_edit_range: None, ime_range: None, ..state.clone() });
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
    TextEditState { selection, last_edit_at: now, coalesce: None, goal_x: None, last_edit_range: None, ime_range: None, ..state.clone() }
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
    let last_edit_range = edits_affected_range(&entry.inverse.edits);
    let (new_value, redo_inverse) = entry.inverse.apply(value);
    let mut redo_stack = state.redo_stack.clone();
    redo_stack.push(UndoEntry { inverse: redo_inverse, selection_before: state.selection.clone() });
    Some((
        new_value,
        TextEditState {
            selection: entry.selection_before, last_edit_at: now, undo_stack, redo_stack,
            coalesce: None, scroll_x: state.scroll_x, goal_x: None, last_edit_range, ime_range: None, ime_origin: None,
            scrolled_cursor: state.scrolled_cursor,
        },
    ))
}

/// Redo the most recently undone edit. `None` when the redo stack is
/// empty (or was cleared by an intervening real edit — standard).
pub fn redo(value: &str, state: &TextEditState, now: f32) -> Option<(String, TextEditState)> {
    let mut redo_stack = state.redo_stack.clone();
    let entry = redo_stack.pop()?;
    let last_edit_range = edits_affected_range(&entry.inverse.edits);
    let (new_value, undo_inverse) = entry.inverse.apply(value);
    let mut undo_stack = state.undo_stack.clone();
    undo_stack.push(UndoEntry { inverse: undo_inverse, selection_before: state.selection.clone() });
    Some((
        new_value,
        TextEditState {
            selection: entry.selection_before, last_edit_at: now, undo_stack, redo_stack,
            coalesce: None, scroll_x: state.scroll_x, goal_x: None, last_edit_range, ime_range: None, ime_origin: None,
            scrolled_cursor: state.scrolled_cursor,
        },
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
        let (v, _ns) = delete_forward("hello", &st(5), 1.0);
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

    // ── D116 Step 5: style_runs, CursorStyle, last_edit_range ────────────

    /// The comparable projection of one `style_runs` output run (`Color`
    /// has no `PartialEq` of its own; `color_bits` is text_edit.rs's
    /// existing local workaround, reused here for the same reason `Span`/
    /// `CursorStyle` need it).
    type RunBits = (usize, usize, Option<(u8, u8, u8, u8)>, Option<FontWeight>);

    fn runs_bits(runs: &[(usize, usize, Option<Color>, Option<FontWeight>)]) -> Vec<RunBits> {
        runs.iter().map(|&(a, b, c, w)| (a, b, c.map(color_bits), w)).collect()
    }

    #[test]
    fn style_runs_with_no_spans_is_one_default_run_covering_the_whole_line() {
        let runs = style_runs(&[], 0, 10);
        assert_eq!(runs_bits(&runs), vec![(0, 10, None, None)]);
    }

    #[test]
    fn style_runs_splits_around_a_span_leaving_default_runs_in_the_gaps() {
        // "hello **world**" -> pretend span covers chars [8, 13) ("world").
        let spans = vec![Span::new((8, 13)).color(Color::rgb(255, 0, 0))];
        let runs = style_runs(&spans, 0, 15);
        assert_eq!(runs_bits(&runs), vec![
            (0, 8, None, None),
            (8, 13, Some((255, 0, 0, 255)), None),
            (13, 15, None, None),
        ]);
    }

    #[test]
    fn style_runs_clips_a_span_that_extends_past_the_requested_range() {
        // Line only covers [0, 5) but the span runs [3, 20) — the run
        // must be clipped to the line's own bounds, not read past it.
        let spans = vec![Span::new((3, 20)).weight(FontWeight::Bold)];
        let runs = style_runs(&spans, 0, 5);
        assert_eq!(runs_bits(&runs), vec![(0, 3, None, None), (3, 5, None, Some(FontWeight::Bold))]);
    }

    #[test]
    fn style_runs_last_matching_span_wins_on_overlap() {
        let spans = vec![
            Span::new((0, 10)).color(Color::rgb(1, 1, 1)),
            Span::new((0, 10)).color(Color::rgb(2, 2, 2)),
        ];
        let runs = style_runs(&spans, 0, 10);
        assert_eq!(runs_bits(&runs), vec![(0, 10, Some((2, 2, 2, 255)), None)]);
    }

    #[test]
    fn style_runs_on_an_empty_range_returns_nothing() {
        assert!(style_runs(&[], 5, 5).is_empty());
    }

    #[test]
    fn cursor_style_default_matches_the_pre_step5_hardcoded_caret() {
        let s = CursorStyle::default();
        assert_eq!(s.width, 1.5);
        assert_eq!(s.blink_rate, 0.53);
        assert_eq!(s.shape, CursorShape::Bar);
    }

    #[test]
    fn typing_sets_last_edit_range_to_just_the_inserted_text_not_the_whole_document() {
        let value = "hello world, this is a long sentence";
        let state = st(value.chars().count());
        let (_, ns) = insert_char(value, &state, '!', 1.0);
        assert_eq!(
            ns.last_edit_range,
            Some((value.chars().count(), value.chars().count() + 1)),
            "an append must report only the newly inserted char's range, not (0, whole_len)"
        );
    }

    #[test]
    fn moving_the_cursor_clears_last_edit_range() {
        let value = "hello";
        let state = st(0);
        let after_type = insert_char(value, &state, 'X', 1.0).1;
        assert!(after_type.last_edit_range.is_some());
        let after_move = move_right(value, &after_type, false, 1.0);
        assert_eq!(after_move.last_edit_range, None, "a pure cursor move is not a content edit");
    }

    // ── D116 Step 6: IME preedit / commit ─────────────────────────────────

    #[test]
    fn ime_preedit_inserts_provisional_text_at_the_cursor() {
        let (v, ns) = ime_set_preedit("hello ", &st(6), "に", None, 1.0);
        assert_eq!(v, "hello に");
        assert_eq!(ns.ime_range, Some((6, 7)));
        assert_eq!(ns.cursor(), 7, "cursor defaults to the end of the preedit text");
    }

    #[test]
    fn ime_preedit_does_not_touch_the_undo_stack() {
        let s = st(0);
        assert!(!s.can_undo());
        let (_, ns) = ime_set_preedit("", &s, "に", None, 1.0);
        assert!(!ns.can_undo(), "a preedit update must not create an undo entry");
    }

    #[test]
    fn a_second_preedit_update_replaces_the_first_not_appends() {
        // Real IME behavior: each keystroke while composing REPLACES the
        // whole provisional buffer with the new romaji->kana candidate,
        // it doesn't insert alongside the old one.
        let (v1, ns1) = ime_set_preedit("", &st(0), "に", None, 1.0);
        assert_eq!(v1, "に");
        let (v2, ns2) = ime_set_preedit(&v1, &ns1, "にほ", None, 1.0);
        assert_eq!(v2, "にほ");
        assert_eq!(ns2.ime_range, Some((0, 2)));
    }

    #[test]
    fn ime_preedit_respects_the_platforms_cursor_position_within_the_text() {
        let (_, ns) = ime_set_preedit("", &st(0), "にほん", Some(1), 1.0);
        assert_eq!(ns.cursor(), 1, "cursor must land where the IME says, not always at the end");
    }

    #[test]
    fn empty_preedit_clears_the_provisional_text_and_range() {
        let (v1, ns1) = ime_set_preedit("", &st(0), "に", None, 1.0);
        let (v2, ns2) = ime_set_preedit(&v1, &ns1, "", None, 1.0);
        assert_eq!(v2, "");
        assert_eq!(ns2.ime_range, None);
    }

    #[test]
    fn ime_commit_finalizes_as_one_real_undoable_edit_and_clears_ime_range() {
        let (v1, ns1) = ime_set_preedit("", &st(0), "に", None, 1.0);
        let (v2, ns2) = ime_set_preedit(&v1, &ns1, "にほ", None, 1.0);
        let (v3, ns3) = ime_commit(&v2, &ns2, "日本", 1.0);
        assert_eq!(v3, "日本");
        assert_eq!(ns3.ime_range, None);
        assert_eq!(ns3.cursor(), char_count("日本"));
        assert!(ns3.can_undo(), "commit must produce a real, undoable edit");

        // The WHOLE composition undoes in ONE step, not one per keystroke.
        let (v4, ns4) = undo(&v3, &ns3, 2.0).expect("commit must be undoable");
        assert_eq!(v4, "");
        assert!(!ns4.can_undo(), "undoing the commit must remove the ONLY undo entry the whole composition produced");
    }

    #[test]
    fn ime_commit_with_no_prior_preedit_replaces_the_selection_like_a_normal_insert() {
        let (v, ns) = ime_commit("hello", &st_sel(1, 3), "X", 1.0);
        assert_eq!(v, "hXlo");
        assert_eq!(ns.cursor(), 2);
    }

    // ── D116 Step 8: input filters ────────────────────────────────────────

    #[test]
    fn apply_filters_with_no_filters_is_a_no_op() {
        assert_eq!(apply_filters("hello", &[]), "hello");
    }

    #[test]
    fn max_length_truncates_from_the_end() {
        let f = [InputFilter::max_length(3)];
        assert_eq!(apply_filters("hello", &f), "hel");
    }

    #[test]
    fn max_length_leaves_a_shorter_value_untouched() {
        let f = [InputFilter::max_length(10)];
        assert_eq!(apply_filters("hi", &f), "hi");
    }

    #[test]
    fn digits_strips_non_digit_characters() {
        let f = [InputFilter::digits()];
        assert_eq!(apply_filters("a1b2c3", &f), "123");
    }

    #[test]
    fn alphanumeric_strips_punctuation_and_spaces() {
        let f = [InputFilter::alphanumeric()];
        assert_eq!(apply_filters("ab! 12-cd", &f), "ab12cd");
    }

    #[test]
    fn custom_char_class_filter() {
        let f = [InputFilter::char_class(|c| c == 'x' || c == 'y')];
        assert_eq!(apply_filters("xayzbx", &f), "xyx");
    }

    #[test]
    fn filters_apply_in_order() {
        // Strip to digits first, THEN clamp to 2 chars.
        let f = [InputFilter::digits(), InputFilter::max_length(2)];
        assert_eq!(apply_filters("a1b2c3", &f), "12");
    }
}
