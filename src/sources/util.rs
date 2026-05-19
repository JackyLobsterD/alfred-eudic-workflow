//! Small shared helpers for source clients.

/// Percent-encode a string for use as a single URL path segment.
/// Encodes every byte not in the RFC 3986 unreserved set
/// (`A-Z a-z 0-9 - _ . ~`), so spaces, slashes, and non-ASCII are safe.
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
}
