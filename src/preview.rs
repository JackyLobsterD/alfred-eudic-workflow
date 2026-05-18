//! Builds a single rich HTML card with the full, untruncated text from
//! every source and writes it to the workflow cache directory. Alfred
//! shows it in a Quick Look popover when the user presses Shift / ⌘Y on
//! any result row (Alfred has no hover/focus popover — Quick Look is the
//! only native "see everything" panel).

use std::fmt::Write as _;
use std::path::Path;

use crate::dictionary::entry::StardictEntry;
use crate::llm::LlmResult;
use crate::sources::DictEntry;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const STYLE: &str = "\
body{font:15px/1.6 -apple-system,Helvetica,sans-serif;background:#1e1e1e;\
color:#e8e8e8;margin:0;padding:22px 26px;}\
h1{font-size:24px;margin:0 0 4px;}\
h2{font-size:15px;margin:22px 0 8px;padding-bottom:4px;\
border-bottom:1px solid #3a3a3a;color:#9ad;}\
.ph{color:#999;margin:2px 0 10px;}\
ol{margin:0;padding-left:22px;}\
li{margin:0 0 10px;}\
.meta{display:block;color:#888;font-size:12px;margin-top:2px;}\
em{color:#c8a;font-style:normal;}\
p{margin:6px 0;}";

/// Render the card to `<cache_dir>/preview.html` and return its absolute
/// path for use as an Alfred `quicklookurl`. Returns `None` if nothing
/// could be written (the caller then simply omits the preview).
pub fn write_preview(
    cache_dir: &Path,
    spell: &str,
    ecdict: Option<&StardictEntry>,
    wordnik: &[DictEntry],
    urban: &[DictEntry],
    llm: Option<&LlmResult>,
) -> Option<String> {
    let mut body = String::new();

    if let Some(e) = ecdict {
        let zh = e
            .translation
            .as_deref()
            .or(e.definition.as_deref())
            .unwrap_or("");
        let _ = write!(body, "<section><h2>📕 ECDICT</h2>");
        if let Some(ph) = e.phonetic.as_deref().filter(|p| !p.is_empty()) {
            let _ = write!(body, "<p class=\"ph\">/{}/</p>", esc(ph));
        }
        let _ = write!(body, "<p>{}</p></section>", esc(zh).replace('\n', "<br>"));
    }

    if !wordnik.is_empty() {
        let _ = write!(body, "<section><h2>📘 Wordnik</h2><ol>");
        for w in wordnik {
            let _ = write!(body, "<li>{}", esc(&w.definition));
            if let Some(x) = w.extra.as_deref().filter(|x| !x.is_empty()) {
                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(x));
            }
            let _ = write!(body, "</li>");
        }
        let _ = write!(body, "</ol></section>");
    }

    if !urban.is_empty() {
        let _ = write!(body, "<section><h2>🔥 Urban Dictionary</h2><ol>");
        for u in urban {
            let def = esc(&u.definition).replace("  e.g. ", "<br><em>e.g. ");
            let _ = write!(body, "<li>{}", def);
            if let Some(x) = u.extra.as_deref().filter(|x| !x.is_empty()) {
                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(x));
            }
            let _ = write!(body, "</li>");
        }
        let _ = write!(body, "</ol></section>");
    }

    if let Some(r) = llm {
        if !r.translations.is_empty() {
            let _ = write!(body, "<section><h2>🤖 Claude 翻译</h2><p>");
            let _ = write!(body, "{}", esc(&r.translations.join("；")));
            let _ = write!(body, "</p>");
            if let Some(ex) = r.example.as_deref().filter(|e| !e.is_empty()) {
                let _ = write!(body, "<p><em>e.g.</em> {}</p>", esc(ex));
            }
            let _ = write!(body, "</section>");
        }
    }

    if body.is_empty() {
        return None;
    }

    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
<style>{STYLE}</style></head><body><h1>{}</h1>{body}</body></html>",
        esc(spell)
    );

    let path = cache_dir.join("preview.html");
    std::fs::write(&path, html).ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_card_with_all_sections_and_escapes() {
        let dir = std::env::temp_dir().join("eudic-preview-test");
        std::fs::create_dir_all(&dir).unwrap();
        let wordnik = vec![DictEntry {
            headword: "x".into(),
            definition: "a <tag> & stuff".into(),
            extra: Some("noun · ahd-5".into()),
        }];
        let urban = vec![DictEntry {
            headword: "x".into(),
            definition: "slang def  e.g. used like this".into(),
            extra: Some("👍 5  👎 1".into()),
        }];
        let llm = LlmResult {
            translations: vec!["甲".into(), "乙".into()],
            example: Some("an example".into()),
        };
        let p = write_preview(&dir, "x<y", None, &wordnik, &urban, Some(&llm)).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        assert!(html.contains("📘 Wordnik"));
        assert!(html.contains("🔥 Urban"));
        assert!(html.contains("🤖 Claude"));
        assert!(html.contains("&lt;tag&gt;")); // HTML escaped
        assert!(html.contains("x&lt;y")); // title escaped
        assert!(html.contains("e.g. used like this")); // urban example split
        assert!(html.contains("甲；乙"));
    }

    #[test]
    fn empty_input_returns_none() {
        let dir = std::env::temp_dir().join("eudic-preview-test");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(write_preview(&dir, "x", None, &[], &[], None).is_none());
    }
}
