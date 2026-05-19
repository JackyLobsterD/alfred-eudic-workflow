use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmExample {
    /// One of: internet, software, casual, office, email, slack
    pub scenario: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmEnglish {
    #[serde(default)]
    pub definitions: Vec<String>,
    #[serde(default)]
    pub examples: Vec<LlmExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmTech {
    /// True iff the word is a tech proper-noun OR has an established
    /// usage in tech/internet/software-development contexts.
    #[serde(default)]
    pub is_tech_term: bool,
    /// Specific tech sub-fields the word appears in. Free-form; model
    /// decides the granularity (e.g. "SQL", "system architecture",
    /// "agent design", "JavaScript", "product", "marketing", ...).
    #[serde(default)]
    pub domains: Vec<String>,
    /// English explanation of the tech meaning(s), if applicable.
    #[serde(default)]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmChinese {
    #[serde(default)]
    pub translations: Vec<String>,
    /// Chinese description of when/how the word is used.
    #[serde(default)]
    pub usage_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmResult {
    #[serde(default)]
    pub english: Option<LlmEnglish>,
    #[serde(default)]
    pub tech: Option<LlmTech>,
    #[serde(default)]
    pub chinese: Option<LlmChinese>,
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
    let result: LlmResult = serde_json::from_str(json).map_err(|e| e.to_string())?;
    // Reject a fully-empty payload — the LLM said nothing useful.
    if result.english.is_none() && result.tech.is_none() && result.chinese.is_none() {
        return Err("LLM result has no english/tech/chinese sections".to_string());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_three_sections() {
        let json = r#"{
          "english": {
            "definitions": ["a fortunate accidental discovery"],
            "examples": [
              {"scenario":"internet","sentence":"What a serendipity in the comments!"},
              {"scenario":"casual","sentence":"What serendipity running into you!"}
            ]
          },
          "tech": {
            "is_tech_term": false,
            "domains": ["product", "team-building"],
            "explanation": "Sometimes used in product retrospectives to label unexpected wins."
          },
          "chinese": {
            "translations": ["机缘巧合", "意外之喜"],
            "usage_notes": "常用于书面或感慨情境，强调好事意外发生。"
          }
        }"#;
        let r = parse_llm_json(json).unwrap();
        let en = r.english.unwrap();
        assert_eq!(en.definitions.len(), 1);
        assert_eq!(en.examples.len(), 2);
        let tech = r.tech.unwrap();
        assert!(!tech.is_tech_term);
        assert_eq!(tech.domains, vec!["product", "team-building"]);
        let zh = r.chinese.unwrap();
        assert_eq!(zh.translations, vec!["机缘巧合", "意外之喜"]);
        assert!(zh.usage_notes.is_some());
    }

    #[test]
    fn parses_partial_chinese_only() {
        let r = parse_llm_json(r#"{"chinese":{"translations":["译"]}}"#).unwrap();
        assert!(r.english.is_none() && r.tech.is_none());
        assert_eq!(r.chinese.unwrap().translations, vec!["译"]);
    }

    #[test]
    fn fenced_json() {
        let r = parse_llm_json("```json\n{\"english\":{\"definitions\":[\"d\"]}}\n```").unwrap();
        assert_eq!(r.english.unwrap().definitions, vec!["d"]);
    }

    #[test]
    fn surrounding_prose() {
        let r = parse_llm_json("Here: {\"chinese\":{\"translations\":[\"译\"]}} done").unwrap();
        assert!(r.chinese.is_some());
    }

    #[test]
    fn rejects_no_braces() {
        assert!(parse_llm_json("hello").is_err());
    }

    #[test]
    fn rejects_all_sections_empty() {
        assert!(parse_llm_json("{}").is_err());
    }
}
