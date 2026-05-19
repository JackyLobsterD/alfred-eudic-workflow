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
use crate::sources::util::{encode_path_segment, strip_tags};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// External web pages for the word, always rendered as a quick-jump bar
/// at the top of the card — even when a source returned no data, the
/// user can click through and look it up directly on the site.
fn external_links(word: &str) -> Vec<(&'static str, String)> {
    let w = encode_path_segment(word.trim());
    vec![
        ("Wikipedia", format!("https://en.wikipedia.org/wiki/{}", w)),
        ("Wiktionary", format!("https://en.wiktionary.org/wiki/{}", w)),
        ("Wordnik", format!("https://www.wordnik.com/words/{}", w)),
        ("M-W Learner's", format!("https://learnersdictionary.com/definition/{}", w)),
        ("M-W Thesaurus", format!("https://www.merriam-webster.com/thesaurus/{}", w)),
        ("Cambridge", format!("https://dictionary.cambridge.org/dictionary/english/{}", w)),
        ("Etymonline", format!("https://www.etymonline.com/word/{}", w)),
        ("Youdao", format!("https://www.youdao.com/result?word={}&lang=en", w)),
    ]
}

const STYLE: &str = "\
body{font:15px/1.6 -apple-system,Helvetica,sans-serif;background:#1e1e1e;\
color:#e8e8e8;margin:0;padding:22px 26px;}\
h1{font-size:24px;margin:0 0 2px;}\
.ph{color:#999;margin:0 0 8px;}\
.links{margin:0 0 18px;font-size:12px;color:#888;}\
.links a{margin-right:14px;white-space:nowrap;}\
h2{font-size:14px;margin:22px 0 6px;padding-bottom:4px;\
border-bottom:1px solid #3a3a3a;color:#9ad;letter-spacing:.3px;}\
h3{font-size:13px;margin:14px 0 4px;color:#cdb;letter-spacing:.3px;}\
h4{font-size:12px;margin:10px 0 2px;color:#a9d;font-weight:600;\
text-transform:none;}\
h4 a{color:#a9d;text-decoration:none;border-bottom:1px dotted #678;}\
h4 a:hover{color:#cdf;}\
ol,ul{margin:0;padding-left:22px;}\
li{margin:0 0 8px;}\
.src{color:#888;font-size:12px;}\
.src a{color:#9ab;}\
.meta{display:block;color:#888;font-size:12px;margin-top:2px;}\
.meta a{color:#9ab;text-decoration:none;border-bottom:1px dotted #555;}\
.meta a:hover{color:#cdf;}\
.tags{color:#c9a;}\
em{color:#c8a;font-style:normal;}\
a{color:#7bf;}\
p{margin:6px 0;}\
.wiki-link{display:inline-block;margin-top:10px;padding:6px 12px;\
background:#2a3b4d;border-radius:4px;color:#9cf;text-decoration:none;\
font-size:13px;}\
.wiki-link:hover{background:#3a4b5d;color:#cdf;}";

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

    // Block 1: English-English — each source gets its own labelled
    // sub-section so the reader can scan by provenance.
    {
        let word_enc = encode_path_segment(spell.trim());
        let wn_url = format!("https://www.wordnik.com/words/{}", word_enc);
        let ml_url = format!("https://learnersdictionary.com/definition/{}", word_enc);
        let wt_url = format!("https://en.wiktionary.org/wiki/{}", word_enc);
        let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", word_enc);

        let has_wn = !wordnik.is_empty();
        let has_ml = extra.mw_learners.as_ref().map(|m| !m.short_defs.is_empty()).unwrap_or(false);
        let has_wk = extra.wiktionary.as_ref().map(|w| !w.senses.is_empty()).unwrap_or(false);
        let has_fd = extra.freedict.as_ref()
            .map(|f| f.meanings.iter().any(|m| !m.definitions.is_empty()))
            .unwrap_or(false);
        let has_yd = extra.youdao.as_ref().map(|y| !y.ee.is_empty()).unwrap_or(false);

        if has_wn || has_ml || has_wk || has_fd || has_yd {
            let _ = write!(body, "<section><h2>🔤 English-English</h2>");

            if has_wn {
                let _ = write!(body, "<h3>📘 <a href=\"{}\">Wordnik →</a></h3><ol>", esc(&wn_url));
                for w in wordnik {
                    let _ = write!(body, "<li>{}", esc(&strip_tags(&w.definition)));
                    if let Some(meta) = w.extra.as_deref().filter(|x| !x.is_empty()) {
                        let _ = write!(body, "<span class=\"meta\">{}</span>", esc(meta));
                    }
                    let _ = write!(body, "</li>");
                }
                let _ = write!(body, "</ol>");
            }

            if let Some(m) = &extra.mw_learners {
                if !m.short_defs.is_empty() {
                    let _ = write!(body, "<h3>📗 <a href=\"{}\">M-W Learner's →</a></h3><ol>", esc(&ml_url));
                    let pos = m.pos.clone().unwrap_or_default();
                    for d in &m.short_defs {
                        let _ = write!(body, "<li>{}", esc(d));
                        if !pos.is_empty() {
                            let _ = write!(body, "<span class=\"meta\">{}</span>", esc(&pos));
                        }
                        let _ = write!(body, "</li>");
                    }
                    let _ = write!(body, "</ol>");
                }
            }

            if let Some(w) = &extra.wiktionary {
                if !w.senses.is_empty() {
                    let _ = write!(body, "<h3>📕 <a href=\"{}\">Wiktionary →</a></h3><ol>", esc(&wt_url));
                    for s in &w.senses {
                        for d in &s.definitions {
                            let _ = write!(body, "<li>{}", esc(d));
                            if !s.pos.is_empty() {
                                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(&s.pos));
                            }
                            let _ = write!(body, "</li>");
                        }
                    }
                    let _ = write!(body, "</ol>");
                }
            }

            if let Some(f) = &extra.freedict {
                if has_fd {
                    let _ = write!(body, "<h3>📓 FreeDict</h3><ol>");
                    for m in &f.meanings {
                        for d in &m.definitions {
                            let _ = write!(body, "<li>{}", esc(d));
                            if !m.pos.is_empty() {
                                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(&m.pos));
                            }
                            let _ = write!(body, "</li>");
                        }
                    }
                    let _ = write!(body, "</ol>");
                }
            }

            if let Some(y) = &extra.youdao {
                if !y.ee.is_empty() {
                    let _ = write!(body, "<h3>🌐 <a href=\"{}\">Youdao →</a></h3><ol>", esc(&yd_url));
                    for line in &y.ee {
                        let _ = write!(body, "<li>{}</li>", esc(line));
                    }
                    let _ = write!(body, "</ol>");
                }
            }

            let _ = write!(body, "</section>");
        }
    }

    // Block 2: synonyms / antonyms / related — every source on its own
    // labelled line; M-W's sense-grouped data renders one line per sense
    // so a 100-synonym thesaurus entry becomes 3 readable groups rather
    // than one wall of text. No content is dropped.
    {
        let mw = extra.mw_thesaurus.as_ref();
        let dm = extra.datamuse.as_ref();
        let fd = extra.freedict.as_ref();
        let yd = extra.youdao.as_ref();

        let mut fd_syn: Vec<String> = Vec::new();
        let mut fd_ant: Vec<String> = Vec::new();
        if let Some(f) = fd {
            for m in &f.meanings {
                fd_syn.extend(m.synonyms.iter().cloned());
                fd_ant.extend(m.antonyms.iter().cloned());
            }
        }
        dedup(&mut fd_syn);
        dedup(&mut fd_ant);

        let has_mw_syn = mw.map(|t| !t.synonym_groups.is_empty()).unwrap_or(false);
        let has_mw_ant = mw.map(|t| !t.antonym_groups.is_empty()).unwrap_or(false);
        let has_dm_syn = dm.map(|d| !d.synonyms.is_empty()).unwrap_or(false);
        let has_dm_ant = dm.map(|d| !d.antonyms.is_empty()).unwrap_or(false);
        let has_dm_rel = dm.map(|d| !d.related.is_empty()).unwrap_or(false);
        let has_fd_syn = !fd_syn.is_empty();
        let has_fd_ant = !fd_ant.is_empty();
        let has_yd_syn = yd.map(|y| !y.syno.is_empty()).unwrap_or(false);
        let has_yd_rel = yd.map(|y| !y.rel_word.is_empty()).unwrap_or(false);

        let any = has_mw_syn || has_mw_ant || has_dm_syn || has_dm_ant
            || has_dm_rel || has_fd_syn || has_fd_ant || has_yd_syn || has_yd_rel;

        // Per-source link targets used as h4 anchors below.
        let w = encode_path_segment(spell.trim());
        let mw_thes_url = format!("https://www.merriam-webster.com/thesaurus/{}", w);
        let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", w);

        if any {
            let _ = write!(body, "<section><h2>🔄 Synonyms / Antonyms / Related</h2>");

            // ---- Synonyms (all sources, source-grouped) ----
            if has_mw_syn || has_dm_syn || has_fd_syn || has_yd_syn {
                let _ = write!(body, "<h3>Synonyms</h3>");
                if let Some(t) = mw {
                    if !t.synonym_groups.is_empty() {
                        let _ = write!(
                            body,
                            "<h4>📘 <a href=\"{}\">M-W Thesaurus →</a></h4>",
                            esc(&mw_thes_url),
                        );
                        for (i, g) in t.synonym_groups.iter().enumerate() {
                            if g.is_empty() { continue; }
                            let label = if t.synonym_groups.len() > 1 {
                                format!("sense {} ({})", i + 1, g.len())
                            } else {
                                format!("({})", g.len())
                            };
                            let _ = write!(
                                body,
                                "<p><span class=\"src\">{}:</span> {}</p>",
                                esc(&label), esc(&g.join(", "))
                            );
                        }
                    }
                }
                if let Some(d) = dm {
                    if !d.synonyms.is_empty() {
                        let _ = write!(body, "<h4>🎯 Datamuse</h4>");
                        let _ = write!(
                            body,
                            "<p><span class=\"src\">({}):</span> {}</p>",
                            d.synonyms.len(), esc(&d.synonyms.join(", "))
                        );
                    }
                }
                if has_fd_syn {
                    let _ = write!(body, "<h4>📕 FreeDict</h4>");
                    let _ = write!(
                        body,
                        "<p><span class=\"src\">({}):</span> {}</p>",
                        fd_syn.len(), esc(&fd_syn.join(", "))
                    );
                }
                if let Some(y) = yd {
                    if !y.syno.is_empty() {
                        let _ = write!(
                            body,
                            "<h4>🌐 <a href=\"{}\">Youdao →</a></h4>",
                            esc(&yd_url),
                        );
                        for line in &y.syno {
                            let _ = write!(body, "<p>{}</p>", esc(line));
                        }
                    }
                }
            }

            // ---- Antonyms (all sources, source-grouped) ----
            if has_mw_ant || has_dm_ant || has_fd_ant {
                let _ = write!(body, "<h3>Antonyms</h3>");
                if let Some(t) = mw {
                    if !t.antonym_groups.is_empty() {
                        let _ = write!(
                            body,
                            "<h4>📘 <a href=\"{}\">M-W Thesaurus →</a></h4>",
                            esc(&mw_thes_url),
                        );
                        for (i, g) in t.antonym_groups.iter().enumerate() {
                            if g.is_empty() { continue; }
                            let label = if t.antonym_groups.len() > 1 {
                                format!("sense {} ({})", i + 1, g.len())
                            } else {
                                format!("({})", g.len())
                            };
                            let _ = write!(
                                body,
                                "<p><span class=\"src\">{}:</span> {}</p>",
                                esc(&label), esc(&g.join(", "))
                            );
                        }
                    }
                }
                if let Some(d) = dm {
                    if !d.antonyms.is_empty() {
                        let _ = write!(body, "<h4>🎯 Datamuse</h4>");
                        let _ = write!(
                            body,
                            "<p><span class=\"src\">({}):</span> {}</p>",
                            d.antonyms.len(), esc(&d.antonyms.join(", "))
                        );
                    }
                }
                if has_fd_ant {
                    let _ = write!(body, "<h4>📕 FreeDict</h4>");
                    let _ = write!(
                        body,
                        "<p><span class=\"src\">({}):</span> {}</p>",
                        fd_ant.len(), esc(&fd_ant.join(", "))
                    );
                }
            }

            // ---- Related / derived ----
            if has_dm_rel || has_yd_rel {
                let _ = write!(body, "<h3>Related</h3>");
                if let Some(d) = dm {
                    if !d.related.is_empty() {
                        let _ = write!(body, "<h4>🎯 Datamuse</h4>");
                        let _ = write!(
                            body,
                            "<p><span class=\"src\">({}):</span> {}</p>",
                            d.related.len(), esc(&d.related.join(", "))
                        );
                    }
                }
                if let Some(y) = yd {
                    if !y.rel_word.is_empty() {
                        let _ = write!(
                            body,
                            "<h4>🌐 <a href=\"{}\">Youdao →</a></h4>",
                            esc(&yd_url),
                        );
                        for line in &y.rel_word {
                            let _ = write!(body, "<p>{}</p>", esc(line));
                        }
                    }
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
            let _ = write!(body, "<section><h2>🧩 Phrases / Examples</h2>");
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
            let _ = write!(body, "<section><h2>📕 Chinese</h2>");
            for line in &zh {
                let _ = write!(body, "<p>{}</p>", esc(line));
            }
            if !web.is_empty() {
                let _ = write!(body, "<p><span class=\"src\">Web:</span> {}</p>", esc(&web.join("; ")));
            }
            let _ = write!(body, "</section>");
        }
    }

    if let Some(e) = ecdict {
        let infl = e.exchange_info();
        let tags = e.tag_info();
        let collins = e.collins.filter(|c| *c > 0).map(|c| "⭐️".repeat(c.min(5) as usize));
        if infl.is_some() || tags.is_some() || collins.is_some() {
            let _ = write!(body, "<section><h2>🔀 Inflections / Tags</h2>");
            if let Some(i) = infl {
                let _ = write!(body, "<p>{}</p>", esc(&i));
            }
            if let Some(t) = tags {
                let _ = write!(body, "<p class=\"tags\">Exam: {}</p>", esc(&t));
            }
            if let Some(c) = collins {
                let _ = write!(body, "<p>Collins {}</p>", c);
            }
            let _ = write!(body, "</section>");
        }
    }

    // Wikipedia: only the official REST API is shown. We deliberately do
    // NOT fall back to Youdao's `wikipedia_digest` — it scrapes the same
    // Wikipedia and ships disambiguation digests like
    // "Splendid may refer to:" with no type field, which the official
    // client correctly rejects (type=disambiguation -> None).
    if let Some(w) = &extra.wikipedia {
        if !w.extract.is_empty() {
            let _ = write!(body, "<section><h2>📖 Wikipedia</h2><p>{}</p>", esc(&w.extract));
            if let Some(u) = &w.url {
                let _ = write!(
                    body,
                    "<a class=\"wiki-link\" href=\"{}\">📖 Read on Wikipedia →</a>",
                    esc(u),
                );
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
            let _ = write!(body, "<section><h2>🌱 Etymology</h2>");
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
            let _ = write!(body, "<section><h2>🔊 Pronunciation</h2>");
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
            let _ = write!(body, "<section><h2>🤖 Claude translation</h2><p>{}</p>", esc(&r.translations.join("; ")));
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

    // Always-on external links bar — even when our card has no data
    // from a given source (rare/misspelled words), the user can click
    // through and look the word up directly on the source's website.
    let mut links_html = String::from("<p class=\"links\">📚 More on ");
    for (name, url) in external_links(spell) {
        let _ = write!(links_html, "· <a href=\"{}\">{}</a> ", esc(&url), esc(name));
    }
    links_html.push_str("</p>");

    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
<style>{STYLE}</style></head><body><h1>{}</h1>{ph}{links}{body}</body></html>",
        esc(spell),
        links = links_html,
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
        // Look for unique section-header markers (with emoji) so we don't
        // collide with the always-on top "More on …" links bar which also
        // contains plain "Wikipedia"/"Wiktionary" link texts.
        let i_ee = html.find("🔤 English-English").unwrap();
        let i_syn = html.find("🔄 Synonyms").unwrap();
        let i_wiki = html.find("📖 Wikipedia").unwrap();
        assert!(i_ee < i_syn && i_syn < i_wiki, "block order must follow priority");
        assert!(html.contains("feeling joy"), "xref stripped");
        assert!(html.contains("ha&lt;ppy"), "title escaped");
        assert!(html.contains("Happiness &lt;is&gt; good"), "wiki escaped");
    }

    #[test]
    fn youdao_wiki_digest_is_never_used_as_fallback() {
        // Regression: Youdao's `wikipedia_digest` ships disambiguation
        // strings like "Splendid may refer to:" without a `type` field.
        // We don't fall back to it — the official Wikipedia client
        // already rejects type=disambiguation as None.
        use crate::sources::youdao::YoudaoData;
        let mut cs = CardSources::default();
        cs.youdao = Some(YoudaoData {
            wiki: Some("Splendid may refer to:".into()),
            ec_zh: vec!["adj. 灿烂的".into()],
            ..Default::default()
        });
        let p = write_preview(&dir(), "splendid", None, &[], &[], None, &cs).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        assert!(!html.contains("📖 Wikipedia"), "wiki block must be absent when only youdao digest exists");
        assert!(!html.contains("Splendid may refer to:"), "disambiguation string must not leak in");
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
        assert!(html.contains("📖 Wikipedia"));
        assert!(!html.contains("🔤 English-English"));
        assert!(!html.contains("🔄 Synonyms"));
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
