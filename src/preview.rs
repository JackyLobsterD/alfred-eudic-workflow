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

/// Filename inside the cache dir for a word's preview card. Per-spell
/// so concurrent queries (e.g. user typing "as" then "ass") cannot
/// race on a single shared file. Non-alphanumeric characters become
/// `_`; ASCII case is normalized; length capped to keep filesystem
/// happy.
pub fn preview_filename(spell: &str) -> String {
    let mut name = String::with_capacity(spell.len() + 12);
    name.push_str("preview-");
    let mut empty = true;
    for c in spell.trim().to_ascii_lowercase().chars().take(60) {
        if c.is_ascii_alphanumeric() {
            name.push(c);
            empty = false;
        } else {
            name.push('_');
        }
    }
    if empty {
        name.push_str("blank");
    }
    name.push_str(".html");
    name
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
.links{margin:0 0 14px;font-size:12px;color:#888;}\
.links a{margin-right:14px;white-space:nowrap;}\
.toc{margin:0 0 22px;padding:10px 12px;background:#262626;\
border-radius:6px;font-size:13px;}\
.toc a{margin-right:14px;white-space:nowrap;color:#9cf;\
text-decoration:none;}\
.toc a:hover{color:#cdf;}\
section{margin-top:30px;padding-top:6px;border-top:1px solid #2a2a2a;}\
section:first-of-type{margin-top:0;border-top:0;padding-top:0;}\
h2{font-size:15px;margin:0 -26px 8px;padding:8px 26px 6px;\
background:#1e1e1e;border-bottom:1px solid #3a3a3a;color:#9ad;\
letter-spacing:.3px;position:sticky;top:0;z-index:5;}\
h3{font-size:13px;margin:14px 0 4px;color:#cdb;letter-spacing:.3px;}\
.sense-row{display:grid;grid-template-columns:max-content 1fr;\
gap:6px 12px;align-items:baseline;margin:4px 0;}\
.sense-row .src{white-space:nowrap;}\
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
.wiki-link:hover{background:#3a4b5d;color:#cdf;}\
.img-grid{display:flex;flex-wrap:wrap;gap:8px;margin:6px 0;}\
.img-grid img{display:block;height:120px;width:auto;border-radius:4px;\
background:#222;}";

/// Render the card to `<cache_dir>/preview.html`; return its path for use
/// as an Alfred `quicklookurl`. `None` if there is nothing to show.
/// When `llm_loading` is true, the `llm` argument is ignored, the Claude
/// section renders a "still thinking" placeholder, and the HTML head
/// gets `<meta http-equiv="refresh" content="2">` so the Quick Look
/// webview auto-reloads until a background subprocess overwrites the
/// file with the finished LLM data.
#[allow(clippy::too_many_arguments)]
pub fn write_preview(
    cache_dir: &Path,
    spell: &str,
    ecdict: Option<&StardictEntry>,
    wordnik: &[DictEntry],
    urban: &[DictEntry],
    llm: Option<&LlmResult>,
    extra: &CardSources,
    llm_loading: bool,
) -> Option<String> {
    let mut body = String::new();

    let phonetic = ecdict
        .and_then(|e| e.phonetic.clone())
        .filter(|p| !p.is_empty())
        .or_else(|| extra.mw_learners.as_ref().and_then(|m| m.phonetic.clone()));

    // 🤖 LLM — rendered FIRST and inline. When the LLM cache is empty
    // we emit a placeholder + a top-level `<meta http-equiv="refresh">`
    // (added in the <head> below). The background `card-update`
    // subprocess later rewrites this entire preview.html with the
    // finished card and no refresh meta, so the reload cycle stops.
    //
    // Quick Look's WebKit refused to honour iframe meta-refresh and
    // appears to disable JavaScript (XHR poller never fired), so the
    // robust path is a top-level meta-refresh. The user accepts the
    // brief whole-page flash on each tick.
    let want_llm = llm_loading
        || llm.map(|r| !render_claude_inner(r).is_empty()).unwrap_or(false);
    if want_llm {
        let inner_now = if llm_loading {
            "<p><span class=\"src\">⏳ Still thinking…</span> \
             this section will appear automatically in ~15–25 seconds. \
             You can keep scrolling the rest of the card.</p>"
                .to_string()
        } else {
            llm.map(render_claude_inner).unwrap_or_default()
        };
        let _ = write!(body, "<section><h2 id=\"llm\">🤖 LLM</h2>{}</section>", inner_now);
    }

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
        let has_yd = extra.youdao.as_ref().map(|y| !y.ee.is_empty()).unwrap_or(false);

        if has_wn || has_ml || has_wk || has_yd {
            let _ = write!(body, "<section><h2 id=\"ee\">🔤 English-English</h2>");

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

            // Wordnik (block above) is shown in full because it's
            // already an aggregated, deduplicated view of multiple
            // sub-sources. The remaining three sources are capped at
            // 5 entries each to keep the section scannable; the user
            // can click each source's link to see the full set on the
            // origin site.
            const SECONDARY_CAP: usize = 5;

            if let Some(m) = &extra.mw_learners {
                if !m.short_defs.is_empty() {
                    let _ = write!(body, "<h3>📗 <a href=\"{}\">M-W Learner's →</a></h3><ol>", esc(&ml_url));
                    let pos = m.pos.clone().unwrap_or_default();
                    for d in m.short_defs.iter().take(SECONDARY_CAP) {
                        let _ = write!(body, "<li>{}", esc(d));
                        if !pos.is_empty() {
                            let _ = write!(body, "<span class=\"meta\">{}</span>", esc(&pos));
                        }
                        let _ = write!(body, "</li>");
                    }
                    if m.short_defs.len() > SECONDARY_CAP {
                        let _ = write!(body, "<li><span class=\"src\">… (+{} more — click M-W Learner's above)</span></li>", m.short_defs.len() - SECONDARY_CAP);
                    }
                    let _ = write!(body, "</ol>");
                }
            }

            if let Some(w) = &extra.wiktionary {
                if !w.senses.is_empty() {
                    let _ = write!(body, "<h3>📕 <a href=\"{}\">Wiktionary →</a></h3><ol>", esc(&wt_url));
                    let mut shown = 0usize;
                    let mut total = 0usize;
                    for s in &w.senses { total += s.definitions.len(); }
                    'wikt: for s in &w.senses {
                        for d in &s.definitions {
                            if shown >= SECONDARY_CAP { break 'wikt; }
                            let _ = write!(body, "<li>{}", esc(d));
                            if !s.pos.is_empty() {
                                let _ = write!(body, "<span class=\"meta\">{}</span>", esc(&s.pos));
                            }
                            let _ = write!(body, "</li>");
                            shown += 1;
                        }
                    }
                    if total > SECONDARY_CAP {
                        let _ = write!(body, "<li><span class=\"src\">… (+{} more — click Wiktionary above)</span></li>", total - SECONDARY_CAP);
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
        let yd = extra.youdao.as_ref();

        let has_mw_syn = mw.map(|t| !t.synonym_groups.is_empty()).unwrap_or(false);
        let has_mw_ant = mw.map(|t| !t.antonym_groups.is_empty()).unwrap_or(false);
        let has_dm_syn = dm.map(|d| !d.synonyms.is_empty()).unwrap_or(false);
        let has_dm_ant = dm.map(|d| !d.antonyms.is_empty()).unwrap_or(false);
        let has_dm_rel = dm.map(|d| !d.related.is_empty()).unwrap_or(false);
        let has_yd_syn = yd.map(|y| !y.syno.is_empty()).unwrap_or(false);
        let has_yd_rel = yd.map(|y| !y.rel_word.is_empty()).unwrap_or(false);

        let any = has_mw_syn || has_mw_ant || has_dm_syn || has_dm_ant
            || has_dm_rel || has_yd_syn || has_yd_rel;

        // Per-source link targets used as h4 anchors below.
        let w = encode_path_segment(spell.trim());
        let mw_thes_url = format!("https://www.merriam-webster.com/thesaurus/{}", w);
        let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", w);

        if any {
            let _ = write!(body, "<section><h2 id=\"syn\">🔄 Synonyms / Antonyms / Related</h2>");

            // ---- Synonyms (all sources, source-grouped) ----
            if has_mw_syn || has_dm_syn || has_yd_syn {
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
                                "<p class=\"sense-row\"><span class=\"src\">{}:</span><span>{}</span></p>",
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
                            "<p class=\"sense-row\"><span class=\"src\">({}):</span><span>{}</span></p>",
                            d.synonyms.len(), esc(&d.synonyms.join(", "))
                        );
                    }
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
            if has_mw_ant || has_dm_ant {
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
                                "<p class=\"sense-row\"><span class=\"src\">{}:</span><span>{}</span></p>",
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
                            "<p class=\"sense-row\"><span class=\"src\">({}):</span><span>{}</span></p>",
                            d.antonyms.len(), esc(&d.antonyms.join(", "))
                        );
                    }
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
                            "<p class=\"sense-row\"><span class=\"src\">({}):</span><span>{}</span></p>",
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
        let has_phr = y.map(|y| !y.phrs.is_empty()).unwrap_or(false);
        let has_sent = y.map(|y| !y.sents.is_empty()).unwrap_or(false);
        if has_phr || has_sent {
            let word_enc = encode_path_segment(spell.trim());
            let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", word_enc);
            let _ = write!(body, "<section><h2 id=\"phrases\">🧩 Phrases / Examples</h2>");
            if let Some(y) = y {
                let _ = write!(body, "<h3>🌐 <a href=\"{}\">Youdao →</a></h3>", esc(&yd_url));
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
            let _ = write!(body, "</section>");
        }
    }

    // Chinese: ECDICT (local) + Youdao (ec_zh translations + web_trans).
    {
        let ec_zh: Option<String> = ecdict
            .and_then(|e| e.translation.as_ref().or(e.definition.as_ref()))
            .map(|t| t.replace('\\', "/").replace('\n', "; "));
        let yd_ec: Vec<String> = extra.youdao.as_ref().map(|y| y.ec_zh.clone()).unwrap_or_default();
        let yd_web: Vec<String> = extra.youdao.as_ref().map(|y| y.web_trans.clone()).unwrap_or_default();
        let has_ecdict = ec_zh.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        let has_yd = !yd_ec.is_empty() || !yd_web.is_empty();
        if has_ecdict || has_yd {
            let word_enc = encode_path_segment(spell.trim());
            let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", word_enc);
            let _ = write!(body, "<section><h2 id=\"zh\">📕 Chinese</h2>");
            if let Some(line) = ec_zh.filter(|s| !s.is_empty()) {
                let _ = write!(body, "<h3>📕 ECDICT</h3><p>{}</p>", esc(&line));
            }
            if has_yd {
                let _ = write!(body, "<h3>🌐 <a href=\"{}\">Youdao →</a></h3>", esc(&yd_url));
                for line in &yd_ec {
                    let _ = write!(body, "<p>{}</p>", esc(line));
                }
                if !yd_web.is_empty() {
                    let _ = write!(
                        body,
                        "<p><span class=\"src\">Web:</span> {}</p>",
                        esc(&yd_web.join("; "))
                    );
                }
            }
            let _ = write!(body, "</section>");
        }
    }

    if let Some(e) = ecdict {
        let infl = e.exchange_info();
        let tags = e.tag_info();
        let collins = e.collins.filter(|c| *c > 0).map(|c| "⭐️".repeat(c.min(5) as usize));
        if infl.is_some() || tags.is_some() || collins.is_some() {
            let _ = write!(body, "<section><h2 id=\"tags\">🔀 Inflections / Tags</h2>");
            let _ = write!(body, "<h3>📕 ECDICT</h3>");
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
            let _ = write!(body, "<section><h2 id=\"wiki\">📖 Wikipedia</h2><p>{}</p>", esc(&w.extract));
            if !w.images.is_empty() {
                let _ = write!(body, "<div class=\"img-grid\">");
                for url in &w.images {
                    let _ = write!(body, "<img src=\"{}\" loading=\"lazy\">", esc(url));
                }
                let _ = write!(body, "</div>");
            }
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

    // Etymology: Youdao etym + FreeDict origin (each rendered under its
    // own source sub-heading).
    {
        let yd_etym = extra.youdao.as_ref().and_then(|y| y.etym.as_deref()).filter(|s| !s.is_empty());
        if let Some(e) = yd_etym {
            let word_enc = encode_path_segment(spell.trim());
            let yd_url = format!("https://www.youdao.com/result?word={}&lang=en", word_enc);
            let _ = write!(body, "<section><h2 id=\"etym\">🌱 Etymology</h2>");
            let _ = write!(
                body,
                "<h3>🌐 <a href=\"{}\">Youdao →</a></h3><p>{}</p>",
                esc(&yd_url), esc(e),
            );
            let _ = write!(body, "</section>");
        }
    }

    // Pronunciation: ECDICT phonetic + M-W Learner's phonetic & audio.
    {
        let ec_ph = ecdict.and_then(|e| e.phonetic.clone()).filter(|s| !s.is_empty());
        let ml_ph = extra.mw_learners.as_ref().and_then(|m| m.phonetic.clone()).filter(|s| !s.is_empty());
        let ml_audio = extra.mw_learners.as_ref().and_then(|m| m.audio_url.clone()).filter(|s| !s.is_empty());
        let any = ec_ph.is_some() || ml_ph.is_some() || ml_audio.is_some();
        if any {
            let _ = write!(body, "<section><h2 id=\"pron\">🔊 Pronunciation</h2>");
            if let Some(p) = &ec_ph {
                let _ = write!(body, "<h3>📕 ECDICT</h3><p>/{}/</p>", esc(p));
            }
            if ml_ph.is_some() || ml_audio.is_some() {
                let word_enc = encode_path_segment(spell.trim());
                let ml_url = format!("https://learnersdictionary.com/definition/{}", word_enc);
                let _ = write!(body, "<h3>📗 <a href=\"{}\">M-W Learner's →</a></h3>", esc(&ml_url));
                if let Some(p) = &ml_ph {
                    let _ = write!(body, "<p>/{}/</p>", esc(p));
                }
                if let Some(a) = &ml_audio {
                    let _ = write!(body, "<p><a href=\"{}\">▶ play audio</a></p>", esc(a));
                }
            }
            let _ = write!(body, "</section>");
        }
    }

    if !urban.is_empty() {
        let _ = write!(body, "<section><h2 id=\"urban\">🔥 Urban Dictionary</h2><ol>");
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

    // While the background subprocess is still fetching the LLM, the
    // whole page auto-reloads every 2 s so the placeholder eventually
    // becomes the finished card. Once the subprocess overwrites this
    // file with `llm_loading=false`, the refresh meta tag disappears
    // and the page stops reloading.
    let refresh_meta = if llm_loading {
        "<meta http-equiv=\"refresh\" content=\"2\">"
    } else {
        ""
    };
    let toc_html = build_toc(&body);
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">{refresh}\
<style>{STYLE}</style></head><body><h1>{}</h1>{ph}{links}{toc}{body}</body></html>",
        esc(spell),
        refresh = refresh_meta,
        links = links_html,
        toc = toc_html,
    );
    // Per-spell filename so concurrent searches (typing "as" then
    // "ass") don't race on a single shared preview.html — each query
    // owns its own file and Quick Look reads the one the current
    // dropdown is bound to.
    let path = cache_dir.join(preview_filename(spell));
    std::fs::write(&path, html).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Sections to surface in the top-of-page table of contents, in render
/// order. Each entry is `(id, short label)`. Only the ids that actually
/// appear in `body` show up in the ToC, so empty sources don't get
/// dangling links.
const TOC_ENTRIES: &[(&str, &str)] = &[
    ("llm",     "🤖 LLM"),
    ("ee",      "🔤 EE"),
    ("syn",     "🔄 Syn/Ant"),
    ("phrases", "🧩 Phrases"),
    ("zh",      "📕 中文"),
    ("tags",    "🔀 Tags"),
    ("wiki",    "📖 Wiki"),
    ("etym",    "🌱 Etym"),
    ("pron",    "🔊 Pron"),
    ("urban",   "🔥 Urban"),
];

fn build_toc(body: &str) -> String {
    let mut links = String::new();
    for (id, label) in TOC_ENTRIES {
        let needle = format!("id=\"{}\"", id);
        if body.contains(&needle) {
            let _ = write!(links, "<a href=\"#{}\">{}</a>", id, label);
        }
    }
    if links.is_empty() {
        String::new()
    } else {
        format!("<nav class=\"toc\">{}</nav>", links)
    }
}

/// Render the inner HTML of the Claude section (the three sub-sections),
/// without the outer `<section>`/`<h2>` wrapper. Returns empty string
/// when the LLM has nothing usable.
fn render_claude_inner(r: &LlmResult) -> String {
    let mut body = String::new();
    let has_en = r.english.as_ref()
        .map(|e| !e.definitions.is_empty() || e.examples.iter().any(|x| !x.sentence.is_empty()))
        .unwrap_or(false);
    let has_tech = r.tech.as_ref()
        .map(|t| t.is_tech_term && (!t.domains.is_empty() || !t.explanation.is_empty()))
        .unwrap_or(false);
    let has_zh = r.chinese.as_ref()
        .map(|c| !c.translations.is_empty() || c.usage_notes.as_deref().map(|s| !s.is_empty()).unwrap_or(false))
        .unwrap_or(false);
    if !has_en && !has_tech && !has_zh {
        return body;
    }
    if let Some(en) = &r.english {
        if has_en {
            let _ = write!(body, "<h3>📖 English meaning</h3>");
            if !en.definitions.is_empty() {
                let _ = write!(body, "<ol>");
                for d in &en.definitions {
                    let _ = write!(body, "<li>{}</li>", esc(d));
                }
                let _ = write!(body, "</ol>");
            }
            let usable: Vec<&crate::llm::response::LlmExample> =
                en.examples.iter().filter(|e| !e.sentence.is_empty()).collect();
            if !usable.is_empty() {
                let _ = write!(body, "<p><span class=\"src\">Examples by register</span></p><ul>");
                for ex in usable {
                    let label = match ex.scenario.as_str() {
                        "internet" => "🌍 Internet",
                        "software" => "💻 Software",
                        "casual"   => "💬 Casual",
                        "office"   => "🏢 Office",
                        "email"    => "✉️ Email",
                        "slack"    => "💬 Slack",
                        other      => other,
                    };
                    let _ = write!(
                        body,
                        "<li><span class=\"src\">{}</span> {}</li>",
                        esc(label),
                        esc(&ex.sentence),
                    );
                }
                let _ = write!(body, "</ul>");
            }
        }
    }
    if let Some(t) = &r.tech {
        if has_tech {
            let _ = write!(body, "<h3>💻 Tech use</h3>");
            if !t.domains.is_empty() {
                let _ = write!(
                    body,
                    "<p><span class=\"src\">Used in:</span> {}</p>",
                    esc(&t.domains.join(" · ")),
                );
            }
            for para in t.explanation.iter().filter(|p| !p.trim().is_empty()) {
                let _ = write!(body, "<p>{}</p>", esc(para));
            }
        }
    }
    if let Some(zh) = &r.chinese {
        if has_zh {
            let _ = write!(body, "<h3>🀄 Chinese</h3>");
            if !zh.translations.is_empty() {
                let _ = write!(body, "<p>{}</p>", esc(&zh.translations.join("; ")));
            }
            if let Some(u) = zh.usage_notes.as_deref().filter(|s| !s.is_empty()) {
                let _ = write!(body, "<p><span class=\"src\">用法:</span> {}</p>", esc(u));
            }
        }
    }
    body
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
    fn toc_skips_missing_sections_and_keeps_present_ones() {
        let body = "<section><h2 id=\"ee\">x</h2></section>\
                    <section><h2 id=\"wiki\">y</h2></section>";
        let toc = build_toc(body);
        assert!(toc.contains("href=\"#ee\""), "ee anchor present");
        assert!(toc.contains("href=\"#wiki\""), "wiki anchor present");
        assert!(!toc.contains("href=\"#syn\""), "absent sections must not appear");
        assert!(toc.contains("class=\"toc\""), "nav wrapper has toc class");
    }

    #[test]
    fn toc_is_empty_when_no_known_sections() {
        assert!(build_toc("<p>nothing</p>").is_empty());
    }

    #[test]
    fn empty_everything_is_none() {
        let cs = CardSources::default();
        assert!(write_preview(&dir(), "x", None, &[], &[], None, &cs, false).is_none());
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
            images: vec![],
        });
        let wordnik = vec![DictEntry {
            headword: "happy".into(),
            definition: "feeling <xref>joy</xref>".into(),
            extra: Some("adjective".into()),
        }];
        let p = write_preview(&dir(), "ha<ppy", None, &wordnik, &[], None, &cs, false).unwrap();
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
        let p = write_preview(&dir(), "splendid", None, &[], &[], None, &cs, false).unwrap();
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
            images: vec![],
        });
        let p = write_preview(&dir(), "t", None, &[], &[], None, &cs, false).unwrap();
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
        let p = write_preview(&dir(), "x", None, &[], &urban, None, &cs, false).unwrap();
        let html = std::fs::read_to_string(&p).unwrap();
        // only ONE example break (from the real separator), the literal text survives
        assert_eq!(html.matches("<br><em>e.g. ").count(), 1);
        assert!(html.contains("plain  e.g. text"));
        assert!(html.contains("an actual example"));
    }

    #[test]
    fn llm_block_skipped_when_translations_empty() {
        let cs = CardSources::default();
        let r = crate::llm::LlmResult { english: None, tech: None, chinese: None };
        // only LLM provided, but empty translations -> nothing to show -> None
        assert!(write_preview(&dir(), "x", None, &[], &[], Some(&r), &cs, false).is_none());
    }
}
