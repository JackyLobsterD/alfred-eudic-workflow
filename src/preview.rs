//! Builds the rich multi-source Quick Look card (Shift / ⌘Y). Blocks are
//! rendered in the user-approved priority order; a block with no data is
//! skipped entirely. The Alfred dropdown list is built elsewhere and is
//! unaffected by this module.

use std::fmt::Write as _;
use std::path::Path;

use crate::card::CardSources;
use crate::dictionary::entry::StardictEntry;
use crate::llm::LlmResult;
use crate::sources::DictEntry;
use crate::sources::util::strip_tags;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

const STYLE: &str = "\
body{font:15px/1.6 -apple-system,Helvetica,sans-serif;background:#1e1e1e;\
color:#e8e8e8;margin:0;padding:22px 26px;}\
h1{font-size:24px;margin:0 0 2px;}\
.ph{color:#999;margin:0 0 14px;}\
h2{font-size:14px;margin:22px 0 8px;padding-bottom:4px;\
border-bottom:1px solid #3a3a3a;color:#9ad;letter-spacing:.3px;}\
ol,ul{margin:0;padding-left:22px;}\
li{margin:0 0 8px;}\
.src{color:#888;font-size:12px;}\
.meta{display:block;color:#888;font-size:12px;margin-top:2px;}\
.tags{color:#c9a;}\
em{color:#c8a;font-style:normal;}\
a{color:#7bf;}\
p{margin:6px 0;}";

/// Render the card to `<cache_dir>/preview.html`; return its path for use
/// as an Alfred `quicklookurl`. `None` if there is nothing to show.
#[allow(clippy::too_many_arguments)]
pub fn write_preview(
    cache_dir: &Path,
    spell: &str,
    ecdict: Option<&StardictEntry>,
    wordnik: &[DictEntry],
    urban: &[DictEntry],
    llm: Option<&LlmResult>,
    extra: &CardSources,
) -> Option<String> {
    let mut body = String::new();

    let phonetic = ecdict
        .and_then(|e| e.phonetic.clone())
        .filter(|p| !p.is_empty())
        .or_else(|| extra.freedict.as_ref().and_then(|f| f.phonetic.clone()))
        .or_else(|| extra.mw_learners.as_ref().and_then(|m| m.phonetic.clone()));

    {
        let mut lines: Vec<(String, String)> = Vec::new();
        for w in wordnik {
            lines.push((strip_tags(&w.definition), format!("Wordnik {}", w.extra.clone().unwrap_or_default())));
        }
        if let Some(m) = &extra.mw_learners {
            let pos = m.pos.clone().unwrap_or_default();
            for d in &m.short_defs {
                lines.push((d.clone(), format!("M-W Learner's {}", pos).trim().to_string()));
            }
        }
        if let Some(y) = &extra.youdao {
            for l in &y.ee {
                lines.push((l.clone(), "有道".to_string()));
            }
        }
        if let Some(w) = &extra.wiktionary {
            for s in &w.senses {
                for d in &s.definitions {
                    lines.push((d.clone(), format!("Wiktionary {}", s.pos)));
                }
            }
        }
        if let Some(f) = &extra.freedict {
            for m in &f.meanings {
                for d in &m.definitions {
                    lines.push((d.clone(), format!("FreeDict {}", m.pos)));
                }
            }
        }
        if !lines.is_empty() {
            let _ = write!(body, "<section><h2>🔤 英英释义 English-English</h2><ol>");
            for (t, src) in lines {
                let _ = write!(body, "<li>{}<span class=\"meta\">{}</span></li>", esc(&t), esc(src.trim()));
            }
            let _ = write!(body, "</ol></section>");
        }
    }

    {
        let mut syn: Vec<String> = Vec::new();
        let mut ant: Vec<String> = Vec::new();
        let mut rel: Vec<String> = Vec::new();
        if let Some(t) = &extra.mw_thesaurus {
            syn.extend(t.synonyms.iter().cloned());
            ant.extend(t.antonyms.iter().cloned());
        }
        if let Some(d) = &extra.datamuse {
            syn.extend(d.synonyms.iter().cloned());
            ant.extend(d.antonyms.iter().cloned());
            rel.extend(d.related.iter().cloned());
        }
        if let Some(f) = &extra.freedict {
            for m in &f.meanings {
                syn.extend(m.synonyms.iter().cloned());
                ant.extend(m.antonyms.iter().cloned());
            }
        }
        let yd_syno = extra.youdao.as_ref().map(|y| (&y.syno, &y.rel_word));
        dedup(&mut syn);
        dedup(&mut ant);
        dedup(&mut rel);
        let has_yd = yd_syno.map(|(s, r)| !s.is_empty() || !r.is_empty()).unwrap_or(false);
        if !syn.is_empty() || !ant.is_empty() || !rel.is_empty() || has_yd {
            let _ = write!(body, "<section><h2>🔄 同义 / 反义 / 联想</h2>");
            if !syn.is_empty() {
                let _ = write!(body, "<p><b>同义</b> {}</p>", esc(&syn.join(", ")));
            }
            if !ant.is_empty() {
                let _ = write!(body, "<p><b>反义</b> {}</p>", esc(&ant.join(", ")));
            }
            if !rel.is_empty() {
                let _ = write!(body, "<p><b>联想</b> {}</p>", esc(&rel.join(", ")));
            }
            if let Some((s, r)) = yd_syno {
                for line in s.iter().chain(r.iter()) {
                    let _ = write!(body, "<p class=\"src\">有道: {}</p>", esc(line));
                }
            }
            let _ = write!(body, "</section>");
        }
    }

    {
        let y = extra.youdao.as_ref();
        let fd = extra.freedict.as_ref();
        let has_phr = y.map(|y| !y.phrs.is_empty()).unwrap_or(false);
        let has_sent = y.map(|y| !y.sents.is_empty()).unwrap_or(false);
        let fd_ex: Vec<&String> = fd
            .map(|f| f.meanings.iter().flat_map(|m| m.examples.iter()).collect())
            .unwrap_or_default();
        if has_phr || has_sent || !fd_ex.is_empty() {
            let _ = write!(body, "<section><h2>🧩 词组短语 / 例句</h2>");
            if let Some(y) = y {
                if !y.phrs.is_empty() {
                    let _ = write!(body, "<ul>");
                    for p in &y.phrs {
                        let _ = write!(body, "<li>{}</li>", esc(p));
                    }
                    let _ = write!(body, "</ul>");
                }
                for (en, zh) in &y.sents {
                    let _ = write!(body, "<p>{}<span class=\"meta\">{}</span></p>", esc(en), esc(zh));
                }
            }
            for ex in fd_ex {
                let _ = write!(body, "<p><em>e.g.</em> {}</p>", esc(ex));
            }
            let _ = write!(body, "</section>");
        }
    }

    {
        let mut zh: Vec<String> = Vec::new();
        if let Some(e) = ecdict {
            if let Some(t) = e.translation.as_ref().or(e.definition.as_ref()) {
                zh.push(t.replace('\\', "/").replace('\n', "; "));
            }
        }
        if let Some(y) = &extra.youdao {
            zh.extend(y.ec_zh.iter().cloned());
        }
        let web: Vec<String> = extra
            .youdao
            .as_ref()
            .map(|y| y.web_trans.clone())
            .unwrap_or_default();
        dedup(&mut zh);
        if !zh.is_empty() || !web.is_empty() {
            let _ = write!(body, "<section><h2>📕 中文释义</h2>");
            for line in &zh {
                let _ = write!(body, "<p>{}</p>", esc(line));
            }
            if !web.is_empty() {
                let _ = write!(body, "<p class=\"src\">网络释义: {}</p>", esc(&web.join("; ")));
            }
            let _ = write!(body, "</section>");
        }
    }

    if let Some(e) = ecdict {
        let infl = e.exchange_info();
        let tags = e.tag_info();
        let collins = e.collins.filter(|c| *c > 0).map(|c| "⭐️".repeat(c.min(5) as usize));
        if infl.is_some() || tags.is_some() || collins.is_some() {
            let _ = write!(body, "<section><h2>🔀 词形变化 / 标签</h2>");
            if let Some(i) = infl {
                let _ = write!(body, "<p>{}</p>", esc(&i));
            }
            if let Some(t) = tags {
                let _ = write!(body, "<p class=\"tags\">考试: {}</p>", esc(&t));
            }
            if let Some(c) = collins {
                let _ = write!(body, "<p>Collins {}</p>", c);
            }
            let _ = write!(body, "</section>");
        }
    }

    {
        let (text, link) = if let Some(w) = &extra.wikipedia {
            (Some(w.extract.clone()), w.url.clone())
        } else if let Some(y) = &extra.youdao {
            (y.wiki.clone(), None)
        } else {
            (None, None)
        };
        if let Some(t) = text.filter(|t| !t.is_empty()) {
            let _ = write!(body, "<section><h2>📖 维基百科</h2><p>{}</p>", esc(&t));
            if let Some(u) = link {
                let _ = write!(body, "<p><a href=\"{}\">{}</a></p>", esc(&u), esc(&u));
            }
            let _ = write!(body, "</section>");
        }
    }

    {
        let mut et: Vec<String> = Vec::new();
        if let Some(y) = &extra.youdao {
            if let Some(e) = &y.etym {
                et.push(e.clone());
            }
        }
        if let Some(f) = &extra.freedict {
            if let Some(o) = &f.origin {
                et.push(o.clone());
            }
        }
        dedup(&mut et);
        if !et.is_empty() {
            let _ = write!(body, "<section><h2>🌱 词源</h2>");
            for e in et {
                let _ = write!(body, "<p>{}</p>", esc(&e));
            }
            let _ = write!(body, "</section>");
        }
    }

    {
        let audio = extra
            .mw_learners
            .as_ref()
            .and_then(|m| m.audio_url.clone())
            .or_else(|| extra.freedict.as_ref().and_then(|f| f.audio.clone()));
        if phonetic.is_some() || audio.is_some() {
            let _ = write!(body, "<section><h2>🔊 发音</h2>");
            if let Some(p) = &phonetic {
                let _ = write!(body, "<p>/{}/</p>", esc(p));
            }
            if let Some(a) = &audio {
                let _ = write!(body, "<p><a href=\"{}\">▶ play audio</a></p>", esc(a));
            }
            let _ = write!(body, "</section>");
        }
    }

    if !urban.is_empty() {
        let _ = write!(body, "<section><h2>🔥 Urban Dictionary</h2><ol>");
        for u in urban {
            let def = esc(&u.definition).replace(crate::sources::URBAN_EXAMPLE_SEP, "<br><em>e.g. ");
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
            let _ = write!(body, "<section><h2>🤖 Claude 翻译</h2><p>{}</p>", esc(&r.translations.join("；")));
            if let Some(ex) = r.example.as_deref().filter(|e| !e.is_empty()) {
                let _ = write!(body, "<p><em>e.g.</em> {}</p>", esc(ex));
            }
            let _ = write!(body, "</section>");
        }
    }

    if body.is_empty() {
        return None;
    }

    let ph = phonetic
        .map(|p| format!("<p class=\"ph\">/{}/</p>", esc(&p)))
        .unwrap_or_default();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
<style>{STYLE}</style></head><body><h1>{}</h1>{ph}{body}</body></html>",
        esc(spell)
    );
    let path = cache_dir.join("preview.html");
    std::fs::write(&path, html).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn dedup(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|s| !s.trim().is_empty() && seen.insert(s.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::datamuse::DatamuseData;
    use crate::sources::wikipedia::WikipediaSummary;

    fn dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let i = N.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("eudic-card-test-{}-{}", std::process::id(), i));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn empty_everything_is_none() {
        let cs = CardSources::default();
        assert!(write_preview(&dir(), "x", None, &[], &[], None, &cs).is_none());
    }

    #[test]
    fn renders_blocks_in_priority_order_and_escapes() {
        let mut cs = CardSources::default();
        cs.datamuse = Some(DatamuseData {
            synonyms: vec!["glad".into()],
            antonyms: vec!["sad".into()],
            related: vec![],
        });
        cs.wikipedia = Some(WikipediaSummary {
            title: "Happy".into(),
            extract: "Happiness <is> good".into(),
            url: Some("https://w/Happy".into()),
        });
        let wordnik = vec![DictEntry {
            headword: "happy".into(),
            definition: "feeling <xref>joy</xref>".into(),
            extra: Some("adjective".into()),
        }];
        let p = write_preview(&dir(), "ha<ppy", None, &wordnik, &[], None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        let i_ee = html.find("英英释义").unwrap();
        let i_syn = html.find("同义").unwrap();
        let i_wiki = html.find("维基百科").unwrap();
        assert!(i_ee < i_syn && i_syn < i_wiki, "block order must follow priority");
        assert!(html.contains("feeling joy"), "xref stripped");
        assert!(html.contains("ha&lt;ppy"), "title escaped");
        assert!(html.contains("Happiness &lt;is&gt; good"), "wiki escaped");
    }

    #[test]
    fn skips_blocks_without_data() {
        let mut cs = CardSources::default();
        cs.wikipedia = Some(WikipediaSummary {
            title: "T".into(),
            extract: "Only wiki here".into(),
            url: None,
        });
        let p = write_preview(&dir(), "t", None, &[], &[], None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        assert!(html.contains("维基百科"));
        assert!(!html.contains("英英释义"));
        assert!(!html.contains("同义"));
    }

    #[test]
    fn urban_example_separator_renders_em_and_no_unclosed_on_literal_text() {
        let cs = CardSources::default();
        let urban = vec![DictEntry {
            headword: "x".into(),
            // a definition that literally contains "  e.g. " must NOT be split
            definition: format!("plain  e.g. text{}an actual example", crate::sources::URBAN_EXAMPLE_SEP),
            extra: None,
        }];
        let p = write_preview(&dir(), "x", None, &[], &urban, None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        // only ONE example break (from the real separator), the literal text survives
        assert_eq!(html.matches("<br><em>e.g. ").count(), 1);
        assert!(html.contains("plain  e.g. text"));
        assert!(html.contains("an actual example"));
    }

    #[test]
    fn llm_block_skipped_when_translations_empty() {
        let cs = CardSources::default();
        let r = crate::llm::LlmResult { translations: vec![], example: Some("ex".into()) };
        // only LLM provided, but empty translations -> nothing to show -> None
        assert!(write_preview(&dir(), "x", None, &[], &[], Some(&r), &cs).is_none());
    }
}
