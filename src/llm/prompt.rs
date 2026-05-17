pub const SYSTEM: &str = "You are a concise bilingual English-Chinese dictionary. \
Detect the input language. Output JSON only, no prose, no markdown fences.";

pub fn user(spell: &str) -> String {
    format!(
        "Word: \"{}\". Output exactly:\n\
         {{\"translations\":[\"释义1\",\"释义2\"],\"example\":\"example sentence\"}}\n\
         Rules: 1-3 translations, each ≤20 Chinese chars or ≤8 English words. \
         Example in the opposite language, ≤15 words. No extra text outside the JSON.",
        spell.replace('"', "'")
    )
}
