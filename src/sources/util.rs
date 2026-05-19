//! Small shared helpers for source clients.

/// Percent-encode a string for use as a single URL path segment.
/// Encodes every byte not in the RFC 3986 unreserved set
/// (`A-Z a-z 0-9 - _ . ~`), so spaces, slashes, and non-ASCII are safe.
/// Remove `<...>` markup, keep inner text, collapse whitespace.
/// A bare `>` that is not closing a tag is preserved (so text like
/// "value > 0" survives).
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            '>' => out.push('>'),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", c));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_space_slash_and_unicode() {
        assert_eq!(encode_path_segment("blood pressure"), "blood%20pressure");
        assert_eq!(encode_path_segment("AC/DC"), "AC%2FDC");
        assert_eq!(encode_path_segment("naïve"), "na%C3%AFve");
        assert_eq!(encode_path_segment("plain"), "plain");
    }

    #[test]
    fn strip_tags_keeps_bare_gt_and_removes_tags() {
        assert_eq!(strip_tags("a <b>bold</b> word"), "a bold word");
        assert_eq!(strip_tags("value > 0 is <i>true</i>"), "value > 0 is true");
        assert_eq!(strip_tags("nested <a href='x'>cupel</a>."), "nested cupel.");
        assert_eq!(strip_tags("plain"), "plain");
    }
}
