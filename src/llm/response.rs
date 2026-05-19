use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmExample {
    /// One of: internet, software, casual, office, email, slack
    pub scenario: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResult {
    pub translations: Vec<String>,
    /// Up to 6 English example sentences in distinct registers.
    #[serde(default)]
    pub examples: Vec<LlmExample>,
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
        let r = parse_llm_json(
            r#"{"translations":["A","B"],"examples":[{"scenario":"casual","sentence":"hi there"}]}"#
        ).unwrap();
        assert_eq!(r.translations, vec!["A", "B"]);
        assert_eq!(r.examples.len(), 1);
        assert_eq!(r.examples[0].scenario, "casual");
        assert_eq!(r.examples[0].sentence, "hi there");
    }

    #[test]
    fn fenced_json_no_examples_field() {
        let r = parse_llm_json("```json\n{\"translations\":[\"A\"]}\n```").unwrap();
        assert_eq!(r.translations, vec!["A"]);
        assert!(r.examples.is_empty());
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
    fn rejects_missing_translations() {
        assert!(parse_llm_json("{\"examples\":[]}").is_err());
    }

    #[test]
    fn parses_six_scenario_examples() {
        let json = r#"{
          "translations":["机缘"],
          "examples":[
            {"scenario":"internet","sentence":"What serendipity in the comments!"},
            {"scenario":"software","sentence":"Discovered a serendipitous bugfix."},
            {"scenario":"casual","sentence":"What serendipity running into you!"},
            {"scenario":"office","sentence":"A serendipitous client introduction."},
            {"scenario":"email","sentence":"This was a serendipitous moment."},
            {"scenario":"slack","sentence":"omg what serendipity"}
          ]
        }"#;
        let r = parse_llm_json(json).unwrap();
        assert_eq!(r.examples.len(), 6);
        let scenarios: Vec<&str> = r.examples.iter().map(|e| e.scenario.as_str()).collect();
        assert_eq!(scenarios, vec!["internet","software","casual","office","email","slack"]);
    }
}
