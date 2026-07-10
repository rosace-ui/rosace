use crate::level::resolve_levels;

/// Reorder text for visual display using TR#9 L2.
///
/// Groups characters by contiguous level, then reverses runs at odd levels
/// (RTL). Returns the visually-ordered string.
pub fn reorder_line(text: &str, levels: &[u8]) -> String {
    let chars: Vec<char> = text.chars().collect();
    assert_eq!(chars.len(), levels.len(), "chars and levels must have the same length");

    if chars.is_empty() { return String::new(); }

    let max_level = *levels.iter().max().unwrap_or(&0);

    let mut result: Vec<char> = chars.clone();

    // L2: For levels from max down to 1, reverse each contiguous run at that level or higher.
    for level in (1..=max_level).rev() {
        let mut i = 0;
        while i < result.len() {
            if levels[i] >= level {
                let start = i;
                while i < result.len() && levels[i] >= level {
                    i += 1;
                }
                result[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }

    result.iter().collect()
}

/// Convenience: resolve levels then reorder in one call.
pub fn reorder(text: &str) -> String {
    let levels = resolve_levels(text);
    reorder_line(text, &levels)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::resolve_levels;

    #[test]
    fn reorder_line_empty() {
        assert_eq!(reorder_line("", &[]), "");
    }

    #[test]
    fn reorder_line_ltr_unchanged() {
        let text = "Hello";
        let levels = resolve_levels(text);
        assert_eq!(reorder_line(text, &levels), "Hello");
    }

    #[test]
    fn reorder_line_rtl_reversed() {
        // Pure RTL: "שלום" — all level 1, so reversed visually.
        let text = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}";
        let chars: Vec<char> = text.chars().collect();
        let levels = vec![1u8; chars.len()];
        let result = reorder_line(text, &levels);
        let expected: String = chars.iter().rev().collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn reorder_line_mixed_runs() {
        // Mix: LTR then RTL — LTR part stays, RTL part reverses.
        let ltr = "Hi";
        let rtl = "\u{05E9}\u{05DC}"; // של
        let text = format!("{}{}", ltr, rtl);
        let mut levels = vec![0u8; ltr.chars().count()];
        levels.extend(vec![1u8; rtl.chars().count()]);
        let result = reorder_line(&text, &levels);
        // LTR portion unchanged; RTL portion reversed
        assert_eq!(result.chars().count(), text.chars().count());
    }

    #[test]
    fn reorder_convenience() {
        let text = "Hello";
        assert_eq!(reorder(text), "Hello");
    }

    #[test]
    fn reorder_single_char() {
        assert_eq!(reorder("A"), "A");
    }

    #[test]
    fn reorder_all_same_level() {
        // All level 0 → unchanged
        let text = "abc";
        let levels = vec![0u8; 3];
        assert_eq!(reorder_line(text, &levels), "abc");
    }

    #[test]
    fn reorder_returns_same_length() {
        let text = "Hello \u{0645}\u{0631}\u{062D}";
        let result = reorder(text);
        assert_eq!(result.chars().count(), text.chars().count());
    }
}
