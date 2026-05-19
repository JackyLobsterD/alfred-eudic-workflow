pub const SYSTEM: &str = "You are a structured trilingual lexicographer that returns three \
sections per word: English meaning + examples, tech-domain usage (if any), and Chinese \
translation + usage notes. Output JSON only — no prose, no markdown fences.";

pub fn user(spell: &str) -> String {
    format!(
        "Word: \"{}\". Output exactly this JSON shape (all three sections; populate every field):\n\
         {{\n\
         \"english\": {{\n\
         \"definitions\": [\"plain-English sense 1\", \"plain-English sense 2 (if any)\"],\n\
         \"examples\": [\n\
         {{\"scenario\":\"internet\",\"sentence\":\"\"}},\n\
         {{\"scenario\":\"software\",\"sentence\":\"\"}},\n\
         {{\"scenario\":\"casual\",\"sentence\":\"\"}},\n\
         {{\"scenario\":\"office\",\"sentence\":\"\"}},\n\
         {{\"scenario\":\"email\",\"sentence\":\"\"}},\n\
         {{\"scenario\":\"slack\",\"sentence\":\"\"}}\n\
         ]\n\
         }},\n\
         \"tech\": {{\n\
         \"is_tech_term\": false,\n\
         \"domains\": [],\n\
         \"explanation\": null\n\
         }},\n\
         \"chinese\": {{\n\
         \"translations\": [\"释义1\",\"释义2\"],\n\
         \"usage_notes\": \"使用场景说明（中文）\"\n\
         }}\n\
         }}\n\n\
         Rules:\n\
         A) ENGLISH section\n\
           - `definitions`: 1-3 plain-English senses, each <=15 words, no jargon.\n\
           - `examples`: all 6 entries, sentences <=20 English words each, using the target word \
         naturally in that register. Keep the 6 scenario keys exactly as listed and in that order.\n\
              · internet: online/social-media post or comment\n\
              · software: software-development context (code review, bug, design, docs)\n\
              · casual: relaxed everyday spoken conversation\n\
              · office: workplace conversation or meeting\n\
              · email: professional email\n\
              · slack: short Slack/IM message (informal, may use lower-case)\n\
         B) TECH section — decide whether this word is a tech proper noun OR has a recognised \
         usage in tech/internet/software-development/product/marketing contexts.\n\
           - If yes: set `is_tech_term` to true. List 1-8 specific sub-domains in `domains` — \
         free-form names, fine-grained (e.g. \"SQL\", \"system architecture\", \"agent design\", \
         \"JavaScript\", \"React\", \"product management\", \"DevOps\", \"SRE\", \"security\", \
         \"data engineering\", \"ML\", \"marketing\", \"UX\", \"growth\", \"crypto\", …). Pick the \
         domains that are most accurate; you decide the granularity. Then fill `explanation` with \
         a concise English description of the tech meaning(s) (<=60 words).\n\
           - If no: set `is_tech_term` to false, `domains` to [], `explanation` to null.\n\
         C) CHINESE section\n\
           - `translations`: 1-3 中文释义 (each <=20 Chinese chars).\n\
           - `usage_notes`: 1-2 句中文，描述这个词常用于什么情境、语体偏正式/口语、有何感情色彩。\n\
         D) Output ONLY the JSON. No prose. No markdown fences. Use null only where explicitly \
         allowed above; never leave a required string empty.",
        spell.replace('"', "'")
    )
}
