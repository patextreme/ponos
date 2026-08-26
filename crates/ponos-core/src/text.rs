//! Shared display-text budgeting.
//!
//! One visible-char budget, shared by prompt lines and tool input peeks
//! (render-logging capability: one truncation constant). Pure string
//! logic, no formatting policy: *what* to truncate is decided by the
//! producers (turn folds, the renderer); this module only provides the
//! how.

/// Visible-char budget shared by prompt lines and tool input peeks
/// (render-logging capability: one truncation constant).
pub const LINE_BUDGET: usize = 120;

/// Truncate to at most `budget` visible characters, marking the cut with
/// a trailing `…`. Unicode-safe: the cut lands on char boundaries
/// (`char_indices`); ANSI sequences never appear in these payloads, so a
/// char cut suffices. Text at or under the budget passes through
/// unchanged.
pub fn truncate_visible(text: &str, budget: usize) -> std::borrow::Cow<'_, str> {
    match text.char_indices().nth(budget) {
        // The (budget+1)-th char exists: cut before its byte offset.
        Some((idx, _)) => std::borrow::Cow::Owned(format!("{}…", &text[..idx])),
        None => std::borrow::Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_visible_short_text_untouched() {
        assert_eq!(truncate_visible("", 120), "");
        assert_eq!(truncate_visible("git status", 120), "git status");
    }

    #[test]
    fn truncate_visible_boundary_lengths() {
        let budget = 8;
        // Exactly the budget: untouched.
        assert_eq!(truncate_visible("12345678", budget), "12345678");
        // One over: the budget's worth of chars plus the marker.
        assert_eq!(truncate_visible("123456789", budget), "12345678…");
        assert_eq!(truncate_visible("1234567890", budget), "12345678…");
    }

    #[test]
    fn truncate_visible_cuts_multi_byte_chars_safely() {
        // 2-byte chars: the cut lands between é's, never mid-codepoint.
        assert_eq!(truncate_visible("éééé", 3), "ééé…");
        // 4-byte emoji likewise.
        assert_eq!(truncate_visible("a😀b😀c", 3), "a😀b…");
        assert_eq!(truncate_visible("😀😀", 2), "😀😀");
    }
}
