//! Shared text-editing primitives (D112/Phase 28 Step 1).
//!
//! Pure, testable operations on a `&str` value + [`TextEditState`] — no
//! rendering, no tree access. The engine's key/click dispatch
//! (`rosace/src/engine.rs`) is the only caller that mutates real state;
//! these functions just compute the next `(value, state)` pair.
//!
//! Positions are CHAR indices (`str::chars()` count), not byte indices —
//! correct for any UTF-8 text (accents, CJK, most emoji) without splitting
//! a codepoint. Full grapheme-cluster correctness (combining marks, ZWJ
//! emoji sequences treated as one editable unit) is a stated, known
//! limitation: no `unicode-segmentation`-class dependency is approved for
//! this crate yet.

use std::sync::Arc;

use rosace_core::types::Rect;

/// Persistent per-node editing chrome (D091): cursor + selection, NOT the
/// text value itself. The value stays app-owned, reported via
/// `EditableDecl::on_change` — the same controlled-component convention
/// every other stateful widget here already uses (`Slider`, `Switch`,
/// `Checkbox`). Cleared only by node removal, never by a repaint — a
/// widget rebuilding with the same value keeps its caret position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEditState {
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    /// `anim_clock()` timestamp of the last edit/move — the caret blink
    /// resets to solid-on from this point, so typing/navigating always
    /// reads as responsive instead of possibly mid-blink-invisible.
    pub last_edit_at: f32,
}

impl TextEditState {
    /// Normalized `(start, end)` char range, `start <= end`. `None` when
    /// there is no active selection (anchor == cursor counts as none).
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.and_then(|a| {
            if a == self.cursor { None } else { Some((a.min(self.cursor), a.max(self.cursor))) }
        })
    }
}

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
}

/// Number of chars in `s` — the coordinate space every position here uses.
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// Byte offset of char index `idx` in `s` (clamped to `s.len()` past the
/// end) — the bridge from char-indexed positions to `&str` slicing.
pub fn char_byte_offset(s: &str, idx: usize) -> usize {
    s.char_indices().nth(idx).map(|(b, _)| b).unwrap_or(s.len())
}

/// Remove the active selection (if any), returning the resulting value and
/// the char index the cursor should land at (the selection's start, or the
/// unchanged cursor when there was nothing selected).
fn delete_selection_raw(value: &str, state: &TextEditState) -> (String, usize) {
    match state.selection_range() {
        Some((s, e)) => {
            let bs = char_byte_offset(value, s);
            let be = char_byte_offset(value, e);
            let mut out = String::with_capacity(value.len() - (be - bs));
            out.push_str(&value[..bs]);
            out.push_str(&value[be..]);
            (out, s)
        }
        None => (value.to_string(), state.cursor),
    }
}

/// Insert `text` at the cursor, replacing any active selection first.
pub fn insert_str(value: &str, state: &TextEditState, text: &str, now: f32) -> (String, TextEditState) {
    let (base, cursor) = delete_selection_raw(value, state);
    let at = char_byte_offset(&base, cursor);
    let mut out = String::with_capacity(base.len() + text.len());
    out.push_str(&base[..at]);
    out.push_str(text);
    out.push_str(&base[at..]);
    let new_cursor = cursor + char_count(text);
    (out, TextEditState { cursor: new_cursor, selection_anchor: None, last_edit_at: now })
}

/// Insert one character — sugar over [`insert_str`] for the common case
/// (`InputEvent::Text` delivers one `char` at a time).
pub fn insert_char(value: &str, state: &TextEditState, ch: char, now: f32) -> (String, TextEditState) {
    let mut buf = [0u8; 4];
    insert_str(value, state, ch.encode_utf8(&mut buf), now)
}

/// Backspace: delete the selection if one is active, else the char before
/// the cursor. No-op at position 0 with no selection.
pub fn backspace(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    if state.selection_range().is_some() {
        let (v, c) = delete_selection_raw(value, state);
        return (v, TextEditState { cursor: c, selection_anchor: None, last_edit_at: now });
    }
    if state.cursor == 0 {
        return (value.to_string(), TextEditState { last_edit_at: now, ..state.clone() });
    }
    let start = char_byte_offset(value, state.cursor - 1);
    let end = char_byte_offset(value, state.cursor);
    let mut out = String::with_capacity(value.len());
    out.push_str(&value[..start]);
    out.push_str(&value[end..]);
    (out, TextEditState { cursor: state.cursor - 1, selection_anchor: None, last_edit_at: now })
}

/// Forward delete: delete the selection if one is active, else the char
/// after the cursor. No-op at the end with no selection.
pub fn delete_forward(value: &str, state: &TextEditState, now: f32) -> (String, TextEditState) {
    if state.selection_range().is_some() {
        let (v, c) = delete_selection_raw(value, state);
        return (v, TextEditState { cursor: c, selection_anchor: None, last_edit_at: now });
    }
    let n = char_count(value);
    if state.cursor >= n {
        return (value.to_string(), TextEditState { last_edit_at: now, ..state.clone() });
    }
    let start = char_byte_offset(value, state.cursor);
    let end = char_byte_offset(value, state.cursor + 1);
    let mut out = String::with_capacity(value.len());
    out.push_str(&value[..start]);
    out.push_str(&value[end..]);
    (out, TextEditState { cursor: state.cursor, selection_anchor: None, last_edit_at: now })
}

/// Move left one char. Without `extend`, an active selection first
/// COLLAPSES to its start (matches every desktop text field's convention —
/// Left doesn't step from wherever the caret glyph happens to render).
pub fn move_left(state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    if !extend {
        if let Some((s, _)) = state.selection_range() {
            return TextEditState { cursor: s, selection_anchor: None, last_edit_at: now };
        }
    }
    let cursor = state.cursor.saturating_sub(1);
    let anchor = if extend { Some(state.selection_anchor.unwrap_or(state.cursor)) } else { None };
    TextEditState { cursor, selection_anchor: anchor, last_edit_at: now }
}

/// Move right one char (collapses an active selection to its end first,
/// symmetric with [`move_left`]).
pub fn move_right(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    if !extend {
        if let Some((_, e)) = state.selection_range() {
            return TextEditState { cursor: e, selection_anchor: None, last_edit_at: now };
        }
    }
    let n = char_count(value);
    let cursor = (state.cursor + 1).min(n);
    let anchor = if extend { Some(state.selection_anchor.unwrap_or(state.cursor)) } else { None };
    TextEditState { cursor, selection_anchor: anchor, last_edit_at: now }
}

pub fn move_home(state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let anchor = if extend { Some(state.selection_anchor.unwrap_or(state.cursor)) } else { None };
    TextEditState { cursor: 0, selection_anchor: anchor, last_edit_at: now }
}

pub fn move_end(value: &str, state: &TextEditState, extend: bool, now: f32) -> TextEditState {
    let n = char_count(value);
    let anchor = if extend { Some(state.selection_anchor.unwrap_or(state.cursor)) } else { None };
    TextEditState { cursor: n, selection_anchor: anchor, last_edit_at: now }
}

pub fn select_all(value: &str, now: f32) -> TextEditState {
    TextEditState { cursor: char_count(value), selection_anchor: Some(0), last_edit_at: now }
}

/// The currently selected substring, or `None` when there is no selection.
pub fn selected_text(value: &str, state: &TextEditState) -> Option<String> {
    state.selection_range().map(|(s, e)| {
        let bs = char_byte_offset(value, s);
        let be = char_byte_offset(value, e);
        value[bs..be].to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(cursor: usize) -> TextEditState {
        TextEditState { cursor, selection_anchor: None, last_edit_at: 0.0 }
    }

    #[test]
    fn insert_char_at_end() {
        let s = st(5);
        let (v, ns) = insert_char("hello", &s, '!', 1.0);
        assert_eq!(v, "hello!");
        assert_eq!(ns.cursor, 6);
        assert_eq!(ns.last_edit_at, 1.0);
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn insert_char_in_middle() {
        let (v, ns) = insert_char("helo", &st(3), 'l', 0.0);
        assert_eq!(v, "hello");
        assert_eq!(ns.cursor, 4);
    }

    #[test]
    fn insert_str_replaces_selection() {
        // "hello world", select "world" (6..11), type "there".
        let s = TextEditState { cursor: 11, selection_anchor: Some(6), last_edit_at: 0.0 };
        let (v, ns) = insert_str("hello world", &s, "there", 2.0);
        assert_eq!(v, "hello there");
        assert_eq!(ns.cursor, 11);
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn insert_handles_multibyte_utf8_without_panicking() {
        // café — 'é' is 2 bytes; cursor=4 is the char index AFTER café.
        let (v, ns) = insert_char("café", &st(4), '!', 0.0);
        assert_eq!(v, "café!");
        assert_eq!(ns.cursor, 5);
    }

    #[test]
    fn backspace_removes_char_before_cursor() {
        let (v, ns) = backspace("hello", &st(5), 1.0);
        assert_eq!(v, "hell");
        assert_eq!(ns.cursor, 4);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let (v, ns) = backspace("hello", &st(0), 1.0);
        assert_eq!(v, "hello");
        assert_eq!(ns.cursor, 0);
    }

    #[test]
    fn backspace_deletes_selection_instead_of_one_char() {
        let s = TextEditState { cursor: 5, selection_anchor: Some(1), last_edit_at: 0.0 };
        let (v, ns) = backspace("hello", &s, 1.0);
        assert_eq!(v, "h");
        assert_eq!(ns.cursor, 1);
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn delete_forward_removes_char_after_cursor() {
        let (v, ns) = delete_forward("hello", &st(0), 1.0);
        assert_eq!(v, "ello");
        assert_eq!(ns.cursor, 0);
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let (v, ns) = delete_forward("hello", &st(5), 1.0);
        assert_eq!(v, "hello");
    }

    #[test]
    fn move_left_decrements_and_clears_selection() {
        let ns = move_left(&st(3), false, 1.0);
        assert_eq!(ns.cursor, 2);
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn move_left_saturates_at_zero() {
        let ns = move_left(&st(0), false, 1.0);
        assert_eq!(ns.cursor, 0);
    }

    #[test]
    fn move_left_without_extend_collapses_selection_to_start() {
        let s = TextEditState { cursor: 5, selection_anchor: Some(1), last_edit_at: 0.0 };
        let ns = move_left(&s, false, 1.0);
        assert_eq!(ns.cursor, 1, "must jump to selection start, not cursor-1");
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn move_right_without_extend_collapses_selection_to_end() {
        let s = TextEditState { cursor: 1, selection_anchor: Some(5), last_edit_at: 0.0 };
        let ns = move_right("hello", &s, false, 1.0);
        assert_eq!(ns.cursor, 5);
        assert!(ns.selection_anchor.is_none());
    }

    #[test]
    fn move_right_extends_selection_from_fresh_anchor() {
        let ns = move_right("hello", &st(2), true, 1.0);
        assert_eq!(ns.cursor, 3);
        assert_eq!(ns.selection_anchor, Some(2), "anchor seeds at the pre-move cursor");
    }

    #[test]
    fn move_right_saturates_at_length() {
        let ns = move_right("hi", &st(2), false, 1.0);
        assert_eq!(ns.cursor, 2);
    }

    #[test]
    fn shift_arrow_sequence_grows_then_shrinks_selection() {
        let s0 = st(2);
        let s1 = move_right("hello world", &s0, true, 0.0);
        let s2 = move_right("hello world", &s1, true, 0.0);
        assert_eq!(s2.selection_range(), Some((2, 4)));
        let s3 = move_left(&s2, true, 0.0);
        assert_eq!(s3.selection_range(), Some((2, 3)));
    }

    #[test]
    fn move_home_and_end() {
        let h = move_home(&st(3), false, 1.0);
        assert_eq!(h.cursor, 0);
        let e = move_end("hello", &st(0), false, 1.0);
        assert_eq!(e.cursor, 5);
    }

    #[test]
    fn select_all_selects_full_range() {
        let s = select_all("hello", 1.0);
        assert_eq!(s.cursor, 5);
        assert_eq!(s.selection_anchor, Some(0));
        assert_eq!(s.selection_range(), Some((0, 5)));
    }

    #[test]
    fn selected_text_extracts_the_right_substring() {
        let s = TextEditState { cursor: 11, selection_anchor: Some(6), last_edit_at: 0.0 };
        assert_eq!(selected_text("hello world", &s).as_deref(), Some("world"));
    }

    #[test]
    fn selected_text_none_when_anchor_equals_cursor() {
        let s = TextEditState { cursor: 3, selection_anchor: Some(3), last_edit_at: 0.0 };
        assert_eq!(selected_text("hello", &s), None);
        assert_eq!(s.selection_range(), None);
    }

    #[test]
    fn selection_range_normalizes_backward_selection() {
        // Shift+Left from position 5 to 2: anchor=5, cursor=2.
        let s = TextEditState { cursor: 2, selection_anchor: Some(5), last_edit_at: 0.0 };
        assert_eq!(s.selection_range(), Some((2, 5)));
    }
}
