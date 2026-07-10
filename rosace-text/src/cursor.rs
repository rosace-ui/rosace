/// A position within a multi-line text layout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextCursor {
    pub line: usize,
    pub col: usize,
}

impl TextCursor {
    pub fn new(line: usize, col: usize) -> Self { Self { line, col } }

    /// Move cursor right by one character, wrapping to next line.
    pub fn advance(&mut self, line_lengths: &[usize]) {
        if self.line >= line_lengths.len() { return; }
        if self.col < line_lengths[self.line] {
            self.col += 1;
        } else if self.line + 1 < line_lengths.len() {
            self.line += 1;
            self.col = 0;
        }
    }

    /// Move cursor left by one character, wrapping to previous line.
    pub fn backspace(&mut self, line_lengths: &[usize]) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = line_lengths.get(self.line).copied().unwrap_or(0);
        }
    }

    /// Move to start of line.
    pub fn home(&mut self) { self.col = 0; }

    /// Move to end of current line.
    pub fn end(&mut self, line_lengths: &[usize]) {
        self.col = line_lengths.get(self.line).copied().unwrap_or(0);
    }

    /// Move up one line (same column or clamped).
    pub fn up(&mut self, line_lengths: &[usize]) {
        if self.line > 0 {
            self.line -= 1;
            self.col = self.col.min(line_lengths.get(self.line).copied().unwrap_or(0));
        }
    }

    /// Move down one line.
    pub fn down(&mut self, line_lengths: &[usize]) {
        if self.line + 1 < line_lengths.len() {
            self.line += 1;
            self.col = self.col.min(line_lengths.get(self.line).copied().unwrap_or(0));
        }
    }

    /// Pixel x-offset of the cursor given a char_width.
    pub fn pixel_x(&self, char_width: f32) -> f32 {
        self.col as f32 * char_width
    }

    /// Pixel y-offset given a line_height.
    pub fn pixel_y(&self, line_height: f32) -> f32 {
        self.line as f32 * line_height
    }
}

/// An anchor+focus pair defining a selected range of text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextSelection {
    pub anchor: TextCursor,
    pub focus: TextCursor,
}

impl TextSelection {
    pub fn new(anchor: TextCursor, focus: TextCursor) -> Self {
        Self { anchor, focus }
    }

    /// True if anchor == focus (collapsed / no selection).
    pub fn is_collapsed(&self) -> bool { self.anchor == self.focus }

    /// Return (start, end) ordered so start <= end by line then col.
    pub fn ordered(&self) -> (&TextCursor, &TextCursor) {
        if self.anchor.line < self.focus.line
        || (self.anchor.line == self.focus.line && self.anchor.col <= self.focus.col) {
            (&self.anchor, &self.focus)
        } else {
            (&self.focus, &self.anchor)
        }
    }

    /// Extract the selected substring from a slice of line strings.
    pub fn text(&self, lines: &[String]) -> String {
        let (start, end) = self.ordered();
        if start.line == end.line {
            let line = lines.get(start.line).map(|s| s.as_str()).unwrap_or("");
            let from = start.col.min(line.len());
            let to   = end.col.min(line.len());
            return line[from..to].to_string();
        }
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i < start.line || i > end.line { continue; }
            if i == start.line {
                result.push_str(&line[start.col.min(line.len())..]);
                result.push('\n');
            } else if i == end.line {
                result.push_str(&line[..end.col.min(line.len())]);
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_new() {
        let c = TextCursor::new(2, 5);
        assert_eq!(c.line, 2);
        assert_eq!(c.col, 5);
    }

    #[test]
    fn cursor_advance_within_line() {
        let mut c = TextCursor::new(0, 0);
        let lengths = [5, 3];
        c.advance(&lengths);
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 1);
    }

    #[test]
    fn cursor_advance_wraps_to_next_line() {
        // col == line_length[0] → wrap
        let mut c = TextCursor::new(0, 5);
        let lengths = [5, 3];
        c.advance(&lengths);
        assert_eq!(c.line, 1);
        assert_eq!(c.col, 0);
    }

    #[test]
    fn cursor_backspace_within_line() {
        let mut c = TextCursor::new(0, 3);
        let lengths = [5, 3];
        c.backspace(&lengths);
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 2);
    }

    #[test]
    fn cursor_backspace_wraps_to_prev_line() {
        let mut c = TextCursor::new(1, 0);
        let lengths = [5, 3];
        c.backspace(&lengths);
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 5);
    }

    #[test]
    fn cursor_home() {
        let mut c = TextCursor::new(1, 4);
        c.home();
        assert_eq!(c.col, 0);
        assert_eq!(c.line, 1);
    }

    #[test]
    fn cursor_end() {
        let mut c = TextCursor::new(0, 1);
        let lengths = [7, 3];
        c.end(&lengths);
        assert_eq!(c.col, 7);
        assert_eq!(c.line, 0);
    }

    #[test]
    fn cursor_up() {
        let mut c = TextCursor::new(1, 3);
        let lengths = [5, 3];
        c.up(&lengths);
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 3); // col <= line 0 length (5), so unchanged
    }

    #[test]
    fn cursor_down() {
        let mut c = TextCursor::new(0, 2);
        let lengths = [5, 3];
        c.down(&lengths);
        assert_eq!(c.line, 1);
        assert_eq!(c.col, 2); // col <= line 1 length (3), so unchanged
    }

    #[test]
    fn cursor_pixel_position() {
        let c = TextCursor::new(2, 4);
        assert!((c.pixel_x(8.0) - 32.0).abs() < 1e-5);
        assert!((c.pixel_y(20.0) - 40.0).abs() < 1e-5);
    }

    // ---- TextSelection tests ----

    #[test]
    fn selection_new() {
        let a = TextCursor::new(0, 1);
        let f = TextCursor::new(0, 5);
        let sel = TextSelection::new(a.clone(), f.clone());
        assert_eq!(sel.anchor, a);
        assert_eq!(sel.focus, f);
    }

    #[test]
    fn selection_collapsed() {
        let c = TextCursor::new(1, 3);
        let sel = TextSelection::new(c.clone(), c.clone());
        assert!(sel.is_collapsed());
    }

    #[test]
    fn selection_is_not_collapsed_when_different() {
        let sel = TextSelection::new(TextCursor::new(0, 0), TextCursor::new(0, 1));
        assert!(!sel.is_collapsed());
    }

    #[test]
    fn selection_ordered_anchor_before_focus() {
        let a = TextCursor::new(0, 2);
        let f = TextCursor::new(0, 5);
        let sel = TextSelection::new(a.clone(), f.clone());
        let (start, end) = sel.ordered();
        assert_eq!(start, &a);
        assert_eq!(end, &f);
    }

    #[test]
    fn selection_ordered_focus_before_anchor() {
        let a = TextCursor::new(1, 3);
        let f = TextCursor::new(0, 5);
        let sel = TextSelection::new(a.clone(), f.clone());
        let (start, end) = sel.ordered();
        assert_eq!(start, &f);
        assert_eq!(end, &a);
    }

    #[test]
    fn selection_text_single_line() {
        let lines = vec!["hello world".to_string()];
        let sel = TextSelection::new(TextCursor::new(0, 6), TextCursor::new(0, 11));
        assert_eq!(sel.text(&lines), "world");
    }

    #[test]
    fn selection_text_empty_when_collapsed() {
        let lines = vec!["hello".to_string()];
        let sel = TextSelection::new(TextCursor::new(0, 2), TextCursor::new(0, 2));
        assert_eq!(sel.text(&lines), "");
    }

    #[test]
    fn selection_text_multi_line() {
        let lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
        ];
        let sel = TextSelection::new(TextCursor::new(0, 6), TextCursor::new(2, 5));
        let text = sel.text(&lines);
        assert!(text.contains("line\n"), "expected 'line\\n' in: {:?}", text);
        assert!(text.contains("second line\n"), "expected 'second line\\n' in: {:?}", text);
        assert!(text.contains("third"), "expected 'third' in: {:?}", text);
    }

    #[test]
    fn selection_text_single_char() {
        let lines = vec!["hello".to_string()];
        let sel = TextSelection::new(TextCursor::new(0, 1), TextCursor::new(0, 2));
        assert_eq!(sel.text(&lines), "e");
    }

    #[test]
    fn selection_ordered_same_line_col() {
        let a = TextCursor::new(2, 4);
        let f = TextCursor::new(2, 4);
        let sel = TextSelection::new(a.clone(), f.clone());
        let (start, end) = sel.ordered();
        assert_eq!(start, &a);
        assert_eq!(end, &f);
    }
}
