// chapter_stats.rs — Deterministic chapter fingerprints (no AI).
//
// Scans every chapter for measurable genre-relevant signals. Stored in
// `chapter_summaries.signals` as JSON when using legacy fingerprint mode, or
// alongside AI prose summaries via `is_prose_summary` detection.

use std::collections::HashMap;

/// Structured per-chapter record — source of truth for deterministic scans.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChapterFingerprint {
    pub schema: String,
    pub title: String,
    pub word_count: usize,
    pub sentence_count: usize,
    pub paragraph_count: usize,
    pub dialogue_pct: u32,
    pub pov: String,
    pub tense: String,
    pub pacing: String,
    /// Lexicon category → occurrence count in chapter text.
    pub lexicon: HashMap<String, u32>,
}

impl ChapterFingerprint {
    pub const SCHEMA: &'static str = "chapter_fingerprint_v1";

    pub fn to_storage_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_storage(raw: &str) -> Option<Self> {
        let fp: Self = serde_json::from_str(raw).ok()?;
        if fp.schema == Self::SCHEMA {
            Some(fp)
        } else {
            None
        }
    }

    /// One-line dossier entry for book-level genre AI (TokenMix).
    pub fn to_dossier_line(&self) -> String {
        let lex = format_lexicon(&self.lexicon);
        let lex_part = if lex.is_empty() {
            String::new()
        } else {
            format!(" Lexicon: {lex}.")
        };
        format!(
            "POV: {}; tense: {}; pacing: {}; ~{}% dialogue; {} words.{}",
            self.pov, self.tense, self.pacing, self.dialogue_pct, self.word_count, lex_part
        )
    }

    /// Short human-readable line for the Chapter Summaries report table.
    pub fn to_display_summary(&self) -> String {
        let lex = format_lexicon(&self.lexicon);
        if lex.is_empty() {
            format!(
                "{} · {} · {}% dialogue · {} words",
                self.pov, self.tense, self.dialogue_pct, self.word_count
            )
        } else {
            format!(
                "{} · {} · {}% dialogue · {} · {} words",
                self.pov, self.tense, self.dialogue_pct, lex, self.word_count
            )
        }
    }
}

pub fn compute_chapter_fingerprint(title: &str, cleaned_text: &str) -> ChapterFingerprint {
    let words: Vec<&str> = cleaned_text.split_whitespace().collect();
    let word_count = words.len();
    let sentence_count = count_sentences(cleaned_text);
    let paragraph_count = count_paragraphs(cleaned_text);

    ChapterFingerprint {
        schema: ChapterFingerprint::SCHEMA.to_string(),
        title: if title.is_empty() {
            "(untitled)".to_string()
        } else {
            title.to_string()
        },
        word_count,
        sentence_count,
        paragraph_count,
        dialogue_pct: dialogue_word_percent(cleaned_text),
        pov: detect_pov(cleaned_text).to_string(),
        tense: detect_tense(cleaned_text).to_string(),
        pacing: pacing_label(word_count, sentence_count, paragraph_count).to_string(),
        lexicon: lexicon_counts(cleaned_text),
    }
}

pub fn aggregate_lexicon(fingerprints: &[ChapterFingerprint]) -> HashMap<String, u32> {
    let mut totals: HashMap<String, u32> = HashMap::new();
    for fp in fingerprints {
        for (k, v) in &fp.lexicon {
            *totals.entry(k.clone()).or_insert(0) += v;
        }
    }
    totals
}

fn format_lexicon(lexicon: &HashMap<String, u32>) -> String {
    let mut pairs: Vec<(String, u32)> = lexicon.iter().map(|(k, v)| (k.clone(), *v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs
        .iter()
        .map(|(k, v)| format!("{k} ({v})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn count_sentences(text: &str) -> usize {
    text.split(['.', '!', '?'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .count()
        .max(1)
}

fn count_paragraphs(text: &str) -> usize {
    let breaks = text.matches("---").count()
        + text.matches("***").count()
        + text.matches("[...").count();
    if breaks > 0 {
        return breaks + 1;
    }
    (count_sentences(text) / 4).max(1)
}

fn dialogue_word_percent(text: &str) -> u32 {
    let total = text.split_whitespace().count();
    if total == 0 {
        return 0;
    }
    let mut in_quote = false;
    let mut dialogue_words = 0usize;
    for word in text.split_whitespace() {
        let opens = word.chars().filter(|&c| c == '"' || c == '\u{201C}').count();
        let closes = word.chars().filter(|&c| c == '"' || c == '\u{201D}').count();
        if in_quote {
            dialogue_words += 1;
        }
        if opens > closes {
            in_quote = true;
            if opens == closes {
                in_quote = false;
            }
        } else if closes > opens {
            in_quote = false;
        } else if opens > 0 && opens == closes {
            dialogue_words += 1;
        }
    }
    ((dialogue_words as f64 / total as f64) * 100.0).round() as u32
}

fn prose_sample(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_quote = false;
    for c in text.chars() {
        match c {
            '"' | '\u{201C}' => in_quote = !in_quote,
            '\u{201D}' => in_quote = false,
            _ if !in_quote => out.push(c),
            _ => {}
        }
    }
    out.to_lowercase()
}

fn detect_pov(text: &str) -> &'static str {
    let sample = format!(" {} ", prose_sample(text));
    let first = count_terms(
        &sample,
        &[" i ", " i'm ", " i've ", " i'd ", " me ", " my ", " mine ", " we ", " our "],
    );
    let third = count_terms(
        &sample,
        &[" he ", " she ", " they ", " him ", " her ", " his ", " hers ", " their "],
    );
    let second = count_terms(&sample, &[" you ", " your ", " yours "]);

    if first > third && first >= second && first > 0 {
        "first person (likely)"
    } else if third > first && third >= second {
        "third person (likely)"
    } else if second > first && second > third {
        "second person (likely)"
    } else {
        "mixed / unclear"
    }
}

fn detect_tense(text: &str) -> &'static str {
    let sample = format!(" {} ", prose_sample(text));
    let past = count_terms(
        &sample,
        &[
            " was ", " were ", " had ", " said ", " walked ", " looked ", " thought ",
            " went ", " came ", " told ", " asked ", " felt ", " knew ", " saw ",
        ],
    );
    let present = count_terms(
        &sample,
        &[
            " is ", " are ", " am ", " has ", " says ", " walks ", " looks ", " thinks ",
            " goes ", " comes ", " tells ", " asks ", " feels ", " knows ", " sees ",
        ],
    );
    if past > present.saturating_mul(2) {
        "past tense (likely)"
    } else if present > past.saturating_mul(2) {
        "present tense (likely)"
    } else {
        "mixed / unclear"
    }
}

fn pacing_label(words: usize, sentences: usize, paragraphs: usize) -> &'static str {
    let avg_sentence = words / sentences.max(1);
    let avg_para = words / paragraphs.max(1);
    if avg_sentence < 12 && words > 800 {
        "fast (short sentences, high word count)"
    } else if avg_sentence > 22 || avg_para > 180 {
        "slow (long sentences or dense blocks)"
    } else {
        "moderate"
    }
}

fn count_terms(haystack: &str, needles: &[&str]) -> usize {
    needles.iter().map(|n| haystack.matches(n).count()).sum()
}

fn lexicon_counts(text: &str) -> HashMap<String, u32> {
    let lower = text.to_lowercase();
    let mut out = HashMap::new();
    for (label, terms) in LEXICONS {
        let n: u32 = terms.iter().map(|t| lower.matches(t).count() as u32).sum();
        if n > 0 {
            out.insert((*label).to_string(), n);
        }
    }
    out
}

const LEXICONS: &[(&str, &[&str])] = &[
    ("romance", &["kiss", "love", "heart", "wedding", "romance", "desire", "attraction", "boyfriend", "girlfriend"]),
    ("faith", &["god", "prayer", "church", "faith", "jesus", "christ", "bible", "worship", "sin", "grace", "pastor"]),
    ("mystery", &["murder", "detective", "clue", "suspect", "investigate", "crime", "alibi", "killer"]),
    ("fantasy", &["magic", "spell", "dragon", "kingdom", "sword", "wizard", "enchant", "elf"]),
    ("thriller", &["gun", "chase", "escape", "threat", "agent", "bomb", "hostage"]),
    ("supernatural", &["ghost", "demon", "vampire", "witch", "curse", "haunted", "possessed"]),
    ("historical", &["century", "regiment", "cottage", "carriage", "empire", "colonial", "wartime"]),
    ("representation", &["intersex", "nonbinary", "transgender", "queer", "lgbt", "asexual"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_round_trip_storage() {
        let fp = compute_chapter_fingerprint("Test", "I walked home and prayed at church.");
        let json = fp.to_storage_json();
        let parsed = ChapterFingerprint::from_storage(&json).unwrap();
        assert_eq!(parsed.word_count, fp.word_count);
        assert!(parsed.lexicon.contains_key("faith"));
    }
}
