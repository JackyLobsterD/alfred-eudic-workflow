pub const SYSTEM: &str = "You are a concise English-to-Chinese dictionary that also shows the \
target word used in distinct registers. Output JSON only, no prose, no markdown fences.";

pub fn user(spell: &str) -> String {
    format!(
        "Word: \"{}\". Output exactly this JSON shape:\n\
         {{\"translations\":[\"释义1\",\"释义2\"],\
         \"examples\":[\
         {{\"scenario\":\"internet\",\"sentence\":\"\"}},\
         {{\"scenario\":\"software\",\"sentence\":\"\"}},\
         {{\"scenario\":\"casual\",\"sentence\":\"\"}},\
         {{\"scenario\":\"office\",\"sentence\":\"\"}},\
         {{\"scenario\":\"email\",\"sentence\":\"\"}},\
         {{\"scenario\":\"slack\",\"sentence\":\"\"}}\
         ]}}\n\
         Rules:\n\
         1) 1-3 Chinese translations (each <=20 Chinese chars).\n\
         2) Provide all 6 example sentences IN ENGLISH (not Chinese), each <=20 words, \
         using the target word naturally in that register:\n\
            - internet: an online/social-media post or comment\n\
            - software: a software-development context (code review, bug, design, docs)\n\
            - casual: relaxed everyday spoken conversation\n\
            - office: a workplace conversation or meeting\n\
            - email: a sentence as it would appear in a professional email\n\
            - slack: a short Slack/IM message (informal, may use lower-case)\n\
         3) Keep scenario keys exactly as listed; preserve their order.\n\
         4) If the word is uncommon, still produce a plausible sentence; never leave a sentence empty.\n\
         5) Output ONLY the JSON, no prose, no markdown fences.",
        spell.replace('"', "'")
    )
}
