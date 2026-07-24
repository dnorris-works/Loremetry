/**
 * Client-side readability metrics — Flesch-Kincaid, Flesch Reading Ease, Gunning Fog.
 * No AI; deterministic formulas on chapter prose.
 */

import type { CachedChapter } from './manuscriptCache';

export interface ReadabilityChapterInput {
  path: string;
  title: string;
  content: string;
}

export interface ChapterReadability {
  chapter_index: number;
  file: string;
  title: string;
  word_count: number;
  sentence_count: number;
  syllable_count: number;
  complex_word_count: number;
  flesch_kincaid_grade: number;
  flesch_reading_ease: number;
  gunning_fog: number;
}

export function stripMarkdownForAnalysis(raw: string): string {
  return raw
    .split('\n')
    .filter(l => !l.trimStart().startsWith('# '))
    .join('\n')
    .replace(/\*\*/g, '')
    .replace(/__/g, '');
}

function extractTitle(content: string): string | null {
  for (const line of content.split('\n').slice(0, 20)) {
    const t = line.trim();
    if (t.startsWith('#')) return t.replace(/^#+\s*/, '');
    if (t) return t;
  }
  return null;
}

/** Heuristic English syllable count (standard readability-tool approach). */
export function countSyllables(word: string): number {
  let w = word.toLowerCase().replace(/[^a-z']/g, '');
  if (!w) return 0;
  if (w.length <= 3) return 1;

  w = w.replace(/(?:[^laeiouy]es|ed|[^laeiouy]e)$/, '');
  w = w.replace(/^y/, '');
  const groups = w.match(/[aeiouy]+/g);
  const count = groups ? groups.length : 1;
  return Math.max(1, count);
}

export function splitSentences(body: string): string[] {
  const parts = body
    .split(/(?<=[.!?]+[\)\]\u201d'\u2019\u201c\u0022]*)\s+/)
    .map(s => s.trim())
    .filter(Boolean);
  if (parts.length > 0) return parts;

  const trimmed = body.trim();
  return trimmed ? [trimmed] : [];
}

export function tokenizeWords(body: string): string[] {
  return body.match(/[A-Za-z']+(?:'[A-Za-z]+)?/g) ?? [];
}

export interface ReadabilityMetrics {
  wordCount: number;
  sentenceCount: number;
  syllableCount: number;
  complexWordCount: number;
  fleschKincaidGrade: number;
  fleschReadingEase: number;
  gunningFog: number;
}

export function analyzeTextReadability(text: string): ReadabilityMetrics {
  const body = stripMarkdownForAnalysis(text).trim();
  const sentences = splitSentences(body);
  const sentenceCount = Math.max(1, sentences.length);
  const words = tokenizeWords(body);
  const wordCount = words.length;

  if (wordCount === 0) {
    return {
      wordCount: 0,
      sentenceCount: 0,
      syllableCount: 0,
      complexWordCount: 0,
      fleschKincaidGrade: 0,
      fleschReadingEase: 0,
      gunningFog: 0,
    };
  }

  let syllableCount = 0;
  let complexWordCount = 0;
  for (const w of words) {
    const syl = countSyllables(w);
    syllableCount += syl;
    if (syl >= 3) complexWordCount += 1;
  }

  const wordsPerSentence = wordCount / sentenceCount;
  const syllablesPerWord = syllableCount / wordCount;

  const fleschKincaidGrade = round1(
    0.39 * wordsPerSentence + 11.8 * syllablesPerWord - 15.59,
  );
  const fleschReadingEase = round1(
    206.835 - 1.015 * wordsPerSentence - 84.6 * syllablesPerWord,
  );
  const gunningFog = round1(
    0.4 * (wordsPerSentence + 100 * (complexWordCount / wordCount)),
  );

  return {
    wordCount,
    sentenceCount,
    syllableCount,
    complexWordCount,
    fleschKincaidGrade: clamp(fleschKincaidGrade, 0, 20),
    fleschReadingEase: clamp(fleschReadingEase, 0, 100),
    gunningFog: clamp(gunningFog, 0, 20),
  };
}

function round1(n: number): number {
  return Math.round(n * 10) / 10;
}

function clamp(n: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, n));
}

export function cachedChapterToInput(c: CachedChapter): ReadabilityChapterInput {
  return {
    path: c.path,
    title: extractTitle(c.content) ?? c.path,
    content: c.content,
  };
}

function easeLabel(score: number): string {
  if (score >= 90) return 'Very easy';
  if (score >= 80) return 'Easy';
  if (score >= 70) return 'Fairly easy';
  if (score >= 60) return 'Standard';
  if (score >= 50) return 'Fairly difficult';
  if (score >= 30) return 'Difficult';
  return 'Very difficult';
}

function gradeLabel(grade: number): string {
  if (grade <= 5) return 'Elementary';
  if (grade <= 8) return 'Middle school';
  if (grade <= 12) return 'High school';
  return 'College';
}

/** Run readability analysis on ordered chapters. Returns readability_v1 JSON object. */
export function runReadabilityAnalysis(chapters: ReadabilityChapterInput[]): Record<string, unknown> {
  if (chapters.length === 0) {
    throw new Error('No chapters to analyze.');
  }

  const rows: ChapterReadability[] = chapters.map((chapter, i) => {
    const fname = chapter.path || `chapter-${i + 1}`;
    const title = chapter.title || extractTitle(chapter.content) || fname;
    const m = analyzeTextReadability(chapter.content);

    return {
      chapter_index: i,
      file: fname,
      title,
      word_count: m.wordCount,
      sentence_count: m.sentenceCount,
      syllable_count: m.syllableCount,
      complex_word_count: m.complexWordCount,
      flesch_kincaid_grade: m.fleschKincaidGrade,
      flesch_reading_ease: m.fleschReadingEase,
      gunning_fog: m.gunningFog,
    };
  });

  const graded = rows.filter(r => r.word_count > 0);
  const grades = graded.map(r => r.flesch_kincaid_grade);
  const eases = graded.map(r => r.flesch_reading_ease);
  const avgGrade = grades.length
    ? round1(grades.reduce((a, b) => a + b, 0) / grades.length)
    : 0;
  const avgEase = eases.length
    ? round1(eases.reduce((a, b) => a + b, 0) / eases.length)
    : 0;
  const minGradeRow = graded.reduce(
    (best, r) => (!best || r.flesch_kincaid_grade < best.flesch_kincaid_grade ? r : best),
    null as ChapterReadability | null,
  );
  const maxGradeRow = graded.reduce(
    (best, r) => (!best || r.flesch_kincaid_grade > best.flesch_kincaid_grade ? r : best),
    null as ChapterReadability | null,
  );

  return {
    schema: 'readability_v1',
    note: 'Deterministic readability formulas (Flesch-Kincaid grade, Flesch Reading Ease, Gunning Fog). Syllables are estimated from spelling — use for chapter-to-chapter comparison within one manuscript, not as a publishing legal standard.',
    summary: {
      total_chapters: rows.length,
      total_words: rows.reduce((s, r) => s + r.word_count, 0),
      avg_flesch_kincaid_grade: avgGrade,
      avg_flesch_reading_ease: avgEase,
      avg_grade_label: gradeLabel(avgGrade),
      avg_ease_label: easeLabel(avgEase),
      min_grade: minGradeRow?.flesch_kincaid_grade ?? 0,
      max_grade: maxGradeRow?.flesch_kincaid_grade ?? 0,
      easiest_chapter: minGradeRow?.title ?? '',
      hardest_chapter: maxGradeRow?.title ?? '',
      grade_spread: round1((maxGradeRow?.flesch_kincaid_grade ?? 0) - (minGradeRow?.flesch_kincaid_grade ?? 0)),
    },
    chapters: rows,
  };
}
