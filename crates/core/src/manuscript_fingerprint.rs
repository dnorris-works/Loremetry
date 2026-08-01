//! Manuscript content hashing and aggregate fingerprints for freshness tracking.

use sha2::{Digest, Sha256};

use crate::documents::{self, Document};

pub fn clean_for_ai(text: &str) -> String {
    fn is_non_visible(c: char) -> bool {
        matches!(
            c,
            '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}' | '\u{1160}'
            | '\u{17B4}' | '\u{17B5}' | '\u{180E}' | '\u{200B}' | '\u{200C}'
            | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{202A}' | '\u{202B}'
            | '\u{202C}' | '\u{202D}' | '\u{202E}' | '\u{2060}' | '\u{2066}'
            | '\u{2067}' | '\u{2068}' | '\u{2069}' | '\u{FEFF}'
        )
    }

    let mut cleaned = String::with_capacity(text.len());
    for c in text.chars() {
        if is_non_visible(c) {
            cleaned.push(' ');
            continue;
        }
        if c.is_control() && !matches!(c, '\n' | '\r' | '\t') {
            cleaned.push(' ');
            continue;
        }
        cleaned.push(c);
    }

    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn chapter_source_hash(cleaned_text: &str) -> String {
    let digest = Sha256::digest(cleaned_text.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub fn chapter_content_hash(content: &str) -> String {
    chapter_source_hash(&clean_for_ai(content))
}

pub fn compute_manuscript_fingerprint(chapters: &[Document]) -> String {
    let mut pairs: Vec<String> = Vec::new();
    for chapter in chapters {
        let fname = documents::chapter_display_name(chapter);
        let cleaned = clean_for_ai(&chapter.content);
        if cleaned.is_empty() {
            continue;
        }
        let hash = chapter_source_hash(&cleaned);
        pairs.push(format!("{fname}:{hash}"));
    }
    let mut hasher = Sha256::new();
    hasher.update(pairs.join("\n").as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub async fn compute_manuscript_fingerprint_for_story(
    pool: &sqlx::PgPool,
    story_id: &str,
) -> String {
    let chapters = documents::list_chapters(pool, story_id)
        .await
        .unwrap_or_default();
    compute_manuscript_fingerprint(&chapters)
}
