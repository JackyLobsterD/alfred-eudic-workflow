use alfred::script_filter::{Item, Mod, Variable};

use crate::dictionary::entry::StardictEntry;
use crate::llm::LlmResult;
use crate::sources::{DictEntry, SourceKind};
use crate::workflow_utils;

const README_CONFIG_URL: &str = "https://github.com/hanleylee/alfred-eudic-workflow#%E5%AE%89%E8%A3%85";
const MAX_SUBTITLE_CHARS: usize = 220;

fn truncate(s: &str) -> String {
    if s.chars().count() <= MAX_SUBTITLE_CHARS { return s.to_string(); }
    let head: String = s.chars().take(MAX_SUBTITLE_CHARS).collect();
    format!("{}…", head)
}

fn source_emoji(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Urban => "🔥",
        SourceKind::Wordnik => "📘",
    }
}

pub fn render_dict(entries: &[DictEntry], kind: SourceKind) -> Vec<Item> {
    entries.iter().enumerate().map(|(i, e)| {
        let prefix = if i == 0 { source_emoji(kind) } else { "  " };
        let title = format!("{} {}", prefix, e.headword);
        let subtitle = workflow_utils::aligned_text(
            &truncate(&e.definition),
            e.extra.as_deref().unwrap_or(""),
        );
        // ⌘ reveals the full (untruncated) definition; ⌥ the metadata
        // (👍/👎 for Urban, part-of-speech · source for Wordnik).
        Item::new(title)
            .subtitle(subtitle)
            .arg(&e.headword)
            .cmd(Mod::new().subtitle(e.definition.clone()))
            .alt(Mod::new().subtitle(e.extra.clone().unwrap_or_default()))
    }).collect()
}

pub fn render_llm(result: &LlmResult, spell: &str) -> Vec<Item> {
    if result.translations.is_empty() {
        return Vec::new();
    }
    // One row: all translations joined. ⌘ repeats them (untruncated),
    // ⌥ shows the example sentence; full text also in the Quick Look card.
    let joined = result.translations.join("；");
    let subtitle = match &result.example {
        Some(ex) => workflow_utils::aligned_text(&joined, ex),
        None => joined.clone(),
    };
    let item = Item::new(format!("🤖 {}", spell))
        .subtitle(subtitle)
        .arg(spell)
        .cmd(Mod::new().subtitle(joined));
    let item = match &result.example {
        Some(ex) => item.alt(Mod::new().subtitle(ex.clone())),
        None => item,
    };
    vec![item]
}

pub fn render_ecdict(entries: &[StardictEntry]) -> Vec<Item> {
    entries.iter().enumerate().map(|(i, entry)| {
        let explanation = entry.translation.as_ref()
            .or(entry.definition.as_ref())
            .map(|s| s.replace('\n', "; "))
            .unwrap_or_default();
        let phonetic = entry.phonetic.as_deref().unwrap_or("");
        let collins_rate = "⭐️".repeat(entry.collins.unwrap_or(0) as usize);
        let mut importance: Vec<String> = Vec::new();
        if let Some(c) = entry.collins { importance.push(format!("COLLINS: {}", "⭐️".repeat(c as usize))); }
        if entry.oxford.is_some() { importance.push("OXFORD 3000".into()); }
        if let Some(bnc) = entry.bnc { if bnc != 0 { importance.push(format!("BNC: {}", bnc)); } }
        if let Some(frq) = entry.frq { if frq != 0 { importance.push(format!("COCA: {}", frq)); } }
        if let Some(tag) = entry.tag_info() { importance.push(tag); }
        let prefix = if i == 0 { "📕" } else { "  " };
        let title = workflow_utils::aligned_text(&format!("{} {}", prefix, entry.word), &collins_rate);
        let subtitle = workflow_utils::aligned_text(&truncate(&explanation), phonetic);
        let cmd_subtitle = entry.exchange_info().unwrap_or_default();
        let alt_subtitle = importance.join("; ");
        Item::new(title)
            .subtitle(subtitle)
            .arg(&entry.word)
            .cmd(Mod::new().subtitle(cmd_subtitle))
            .alt(Mod::new().subtitle(alt_subtitle))
    }).collect()
}

pub fn render_no_api_key(source_name: &str) -> Item {
    Item::new(format!("⚙️ {} 未配置 API key", source_name))
        .subtitle("回车查看配置说明")
        .arg(README_CONFIG_URL)
}

pub fn render_error(source_name: &str, err_msg: &str, spell: &str) -> Item {
    Item::new(format!("⚠️ {}: {}", source_name, err_msg))
        .subtitle("回车重试（绕过缓存）")
        .arg(spell)
        .variable(Variable::new(Some("BYPASS_CACHE".into()), Some("1".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_first_item_has_emoji_prefix() {
        let entries = vec![
            DictEntry { headword: "rizz".into(), definition: "charisma".into(), extra: None },
            DictEntry { headword: "rizz".into(), definition: "skill".into(), extra: None },
        ];
        let items = render_dict(&entries, SourceKind::Urban);
        let json = serde_json::to_value(&items[0]).unwrap();
        let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("");
        assert!(title.starts_with("🔥"));
        let json2 = serde_json::to_value(&items[1]).unwrap();
        let title2 = json2.get("title").and_then(|v| v.as_str()).unwrap_or("");
        assert!(!title2.starts_with("🔥"));
    }

    #[test]
    fn llm_renders_translations() {
        let r = LlmResult {
            translations: vec!["机缘".to_string(), "巧合".to_string()],
            example: Some("What a serendipity!".to_string()),
        };
        let items = render_llm(&r, "serendipity");
        assert_eq!(items.len(), 1);
        let json = serde_json::to_value(&items[0]).unwrap();
        let subtitle = json.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
        assert!(subtitle.contains("机缘"));
        assert!(subtitle.contains("巧合"));
    }

    #[test]
    fn error_item_sets_bypass_variable() {
        let item = render_error("Wordnik", "timeout", "rizz");
        let json = serde_json::to_value(&item).unwrap();
        let vars = json.get("variables").cloned().unwrap_or_default();
        assert_eq!(vars.get("BYPASS_CACHE").and_then(|v| v.as_str()), Some("1"));
    }

    #[test]
    fn long_definition_is_truncated() {
        let long = "a".repeat(500);
        let entries = vec![DictEntry { headword: "x".into(), definition: long, extra: None }];
        let items = render_dict(&entries, SourceKind::Wordnik);
        let json = serde_json::to_value(&items[0]).unwrap();
        let subtitle = json.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
        assert!(subtitle.contains('…'));
    }
}
