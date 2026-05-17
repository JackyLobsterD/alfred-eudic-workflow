use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResult {
    pub translations: Vec<String>,
    #[serde(default)]
    pub example: Option<String>,
}

/// Extract a JSON object from possibly-noisy model output.
/// Handles: clean JSON, JSON in code fences, JSON surrounded by prose.
pub fn parse_llm_json(raw: &str) -> Result<LlmResult, String> {
    let trimmed = raw.trim();
    // Strip ```json ... ``` fences if present.
    let stripped = if let Some(rest) = trimmed.strip_prefix("```") {
        let after_lang = rest.splitn(2, '\n').nth(1).unwrap_or(rest);
        after_lang.trim_end_matches("```").trim()
    } else {
        trimmed
    };
    // Find the outermost {...}.
    let start = stripped.find('{').ok_or_else(|| "no '{' in output".to_string())?;
    let end = stripped.rfind('}').ok_or_else(|| "no '}' in output".to_string())?;
    if end < start {
        return Err("malformed JSON braces".to_string());
    }
    let json = &stripped[start..=end];
    serde_json::from_str::<LlmResult>(json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_json() {
        let r = parse_llm_json(r#"{"translations":["A","B"],"example":"x"}"#).unwrap();
        assert_eq!(r.translations, vec!["A", "B"]);
        assert_eq!(r.example.as_deref(), Some("x"));
    }

    #[test]
    fn fenced_json() {
        let r = parse_llm_json("```json\n{\"translations\":[\"A\"]}\n```").unwrap();
        assert_eq!(r.translations, vec!["A"]);
        assert_eq!(r.example, None);
    }

    #[test]
    fn surrounding_prose() {
        let r = parse_llm_json("Here you go: {\"translations\":[\"A\"]} done").unwrap();
        assert_eq!(r.translations, vec!["A"]);
    }

    #[test]
    fn rejects_no_braces() {
        assert!(parse_llm_json("hello").is_err());
    }

    #[test]
    fn rejects_missing_field() {
        assert!(parse_llm_json("{\"example\":\"x\"}").is_err());
    }
}
