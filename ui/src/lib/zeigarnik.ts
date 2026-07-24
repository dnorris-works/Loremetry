/**
 * Client-side Zeigarnik effect proxy detector — port of crates/core/src/analysis/zeigarnik.rs.
 * Deterministic pattern matching only; no AI.
 */

import zeigarnikConfig from '../data/zeigarnik-config.json';
import type { CachedChapter } from './manuscriptCache';

export interface ZeigarnikConfig {
  cliffhanger_markers: string[];
  resolution_markers: string[];
  question_lead_ins: string[];
  short_fragment_max_words: number;
  min_gap_chapters_for_thread: number;
  max_total_mentions_for_thread: number;
  min_thread_term_len: number;
  top_threads_limit: number;
  min_question_words: number;
  max_questions_per_chapter: number;
}

export interface ZeigarnikChapterInput {
  path: string;
  title: string;
  content: string;
}

interface ChapterRow {
  chapter_index: number;
  file: string;
  title: string;
  word_count: number;
  sentence_count: number;
  question_count: number;
  ending_type: string;
  tension_score: number;
  ending_snippet: string;
}

interface ThreadRow {
  term: string;
  mention_count: number;
  first_chapter_index: number;
  first_file: string;
  first_snippet: string;
  gap_start_index: number;
  gap_end_index: number;
  max_gap_chapters: number;
  max_gap_words: number;
}

interface Occurrence {
  chapterIdx: number;
  sentenceInitial: boolean;
  snippet: string;
}

const STOPWORDS = new Set([
  'The', 'A', 'An', 'He', 'She', 'It', 'They', 'We', 'I', 'But', 'And', 'So',
  'Then', 'When', 'If', 'As', 'You', 'Chapter', 'There', 'Here', 'This', 'That', 'Her', 'His',
]);

function loadConfig(): ZeigarnikConfig {
  const t = zeigarnikConfig.thresholds;
  return {
    cliffhanger_markers: zeigarnikConfig.cliffhanger_markers,
    resolution_markers: zeigarnikConfig.resolution_markers,
    question_lead_ins: zeigarnikConfig.question_lead_ins,
    short_fragment_max_words: t.short_fragment_max_words,
    min_gap_chapters_for_thread: t.min_gap_chapters_for_thread,
    max_total_mentions_for_thread: t.max_total_mentions_for_thread,
    min_thread_term_len: t.min_thread_term_len,
    top_threads_limit: t.top_threads_limit,
    min_question_words: t.min_question_words,
    max_questions_per_chapter: t.max_questions_per_chapter,
  };
}

function stripMarkdown(raw: string): string {
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
    if (t.startsWith('#')) {
      return t.replace(/^#+\s*/, '');
    }
    if (t) return t;
  }
  return null;
}

function lastParagraph(body: string): string {
  const parts = body.split('\n\n').map(p => p.trim()).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : '';
}

function splitSentences(body: string): string[] {
  const re = /[^.!?]*[.!?]+[\)\]\u201d'\u2019\u201c\u0022]*/gu;
  const out: string[] = [];
  let matchedLen = 0;
  for (const m of body.matchAll(re)) {
    const s = m[0].trim();
    if (s) {
      out.push(s);
      matchedLen += s.length;
    }
  }
  const remainder = body.trim();
  if (remainder.length > matchedLen + 20) {
    const tail = remainder.split(/[.!?]/).pop()?.trim() ?? '';
    if (tail.split(/\s+/).length >= 3) {
      out.push(tail);
    }
  }
  return out;
}

function detectEnding(
  lastPara: string,
  sentences: string[],
  config: ZeigarnikConfig,
): { endingType: string; tensionScore: number; endingSnippet: string } {
  const lower = lastPara.toLowerCase();
  let score = 35;

  const lastSentence = sentences.length ? sentences[sentences.length - 1] : '';
  const lastLower = lastSentence.toLowerCase();

  if (lastLower.trimEnd().endsWith('?')) score += 25;
  if (lastPara.trimEnd().endsWith('\u2014') || lastPara.trimEnd().endsWith('...') || lastPara.trimEnd().endsWith('\u2026')) {
    score += 15;
  }
  if (config.cliffhanger_markers.some(m => lower.includes(m))) score += 25;
  if (config.resolution_markers.some(m => lower.includes(m))) score -= 35;

  if (sentences.length >= 2) {
    const lastWords = lastSentence.split(/\s+/).length;
    const priorWords = sentences[sentences.length - 2].split(/\s+/).length;
    if (lastWords > 0 && lastWords <= config.short_fragment_max_words && priorWords > lastWords * 2) {
      score += 12;
    }
  }

  score = Math.max(0, Math.min(100, score));
  const endingType = score >= 65 ? 'cliffhanger' : score <= 25 ? 'resolved' : 'neutral';

  const take = sentences.length >= 2 ? 2 : Math.min(sentences.length, 1);
  let snippet = sentences.slice(Math.max(0, sentences.length - take)).join(' ');
  if (!snippet) snippet = [...lastPara].slice(0, 200).join('');
  if ([...snippet].length > 240) {
    snippet = [...snippet].slice(0, 240).join('') + '…';
  }

  return { endingType, tensionScore: score, endingSnippet: snippet };
}

function extractOpenQuestions(sentences: string[], config: ZeigarnikConfig): string[] {
  const candidates: { leadIn: boolean; text: string }[] = [];
  for (const s of sentences) {
    if (!s.trimEnd().endsWith('?')) continue;
    const wordCount = s.split(/\s+/).length;
    if (wordCount < config.min_question_words) continue;
    const lower = s.toLowerCase();
    const isLeadIn = config.question_lead_ins.some(p => lower.includes(p));
    candidates.push({ leadIn: isLeadIn, text: s.trim() });
  }
  candidates.sort((a, b) => Number(b.leadIn) - Number(a.leadIn));
  return candidates.slice(0, config.max_questions_per_chapter).map(c => c.text);
}

function scanEntityOccurrences(chapterTexts: string[]): Map<string, Occurrence[]> {
  const capWord = String.raw`[A-Z][a-zA-Z'\u2019]+`;
  const re = new RegExp(String.raw`(?:${capWord})(?:\s+(?:${capWord})){0,2}`, 'g');
  const map = new Map<string, Occurrence[]>();

  chapterTexts.forEach((text, chapterIdx) => {
    for (const m of text.matchAll(re)) {
      const raw = m[0];
      const words = raw.split(/\s+/);
      const isMultiword = words.length > 1;
      if (!isMultiword && STOPWORDS.has(raw)) continue;

      const start = m.index ?? 0;
      const before = text.slice(0, start).trimEnd();
      const lastChar = before.length ? before[before.length - 1] : null;
      const sentenceInitial = lastChar === null
        || lastChar === '.' || lastChar === '!' || lastChar === '?'
        || lastChar === '\u201c' || lastChar === '\u2018' || lastChar === '"';

      const norm = raw.toLowerCase();
      const ctxStart = Math.max(0, start - 40);
      const ctxEnd = Math.min(text.length, start + raw.length + 40);
      const snippet = `…${text.slice(ctxStart, ctxEnd).trim()}…`;

      const list = map.get(norm) ?? [];
      list.push({ chapterIdx, sentenceInitial, snippet });
      map.set(norm, list);
    }
  });

  return map;
}

function titleCase(s: string): string {
  return s.split(/\s+/).map(w => {
    if (!w) return '';
    return w[0].toUpperCase() + w.slice(1);
  }).join(' ');
}

function findOpenThreads(
  occurrences: Map<string, Occurrence[]>,
  chapterRows: ChapterRow[],
  config: ZeigarnikConfig,
): ThreadRow[] {
  const threads: ThreadRow[] = [];

  for (const [term, occs] of occurrences) {
    if (term.length < config.min_thread_term_len) continue;

    const isMultiword = term.includes(' ');
    const hasNonInitial = occs.some(o => !o.sentenceInitial);
    if (!isMultiword && !hasNonInitial) continue;

    let chapterIdxs = [...new Set(occs.map(o => o.chapterIdx))].sort((a, b) => a - b);
    if (chapterIdxs.length < 2) continue;
    if (chapterIdxs.length > config.max_total_mentions_for_thread) continue;

    let maxGap = 0;
    let gapStart = 0;
    let gapEnd = 0;
    for (let i = 0; i < chapterIdxs.length - 1; i++) {
      const g = chapterIdxs[i + 1] - chapterIdxs[i];
      if (g > maxGap) {
        maxGap = g;
        gapStart = chapterIdxs[i];
        gapEnd = chapterIdxs[i + 1];
      }
    }
    if (maxGap < config.min_gap_chapters_for_thread) continue;

    const gapWords = chapterRows
      .filter(c => c.chapter_index > gapStart && c.chapter_index < gapEnd)
      .reduce((sum, c) => sum + c.word_count, 0);

    const firstOcc = occs.find(o => o.chapterIdx === chapterIdxs[0]);
    const firstFile = chapterRows[chapterIdxs[0]]?.file ?? '';

    threads.push({
      term: titleCase(term),
      mention_count: chapterIdxs.length,
      first_chapter_index: chapterIdxs[0],
      first_file: firstFile,
      first_snippet: firstOcc?.snippet ?? '',
      gap_start_index: gapStart,
      gap_end_index: gapEnd,
      max_gap_chapters: maxGap,
      max_gap_words: gapWords,
    });
  }

  threads.sort((a, b) =>
    b.max_gap_chapters - a.max_gap_chapters || b.max_gap_words - a.max_gap_words,
  );
  return threads.slice(0, config.top_threads_limit);
}

export function cachedChapterToInput(c: CachedChapter): ZeigarnikChapterInput {
  return {
    path: c.path,
    title: extractTitle(c.content) ?? c.path,
    content: c.content,
  };
}

/** Run Zeigarnik analysis on ordered chapter inputs. Returns zeigarnik_v1 JSON object. */
export function runZeigarnikAnalysis(chapters: ZeigarnikChapterInput[]): Record<string, unknown> {
  if (chapters.length === 0) {
    throw new Error('No chapters to analyze.');
  }

  const config = loadConfig();
  const chapterTexts: string[] = [];
  const chapterRows: ChapterRow[] = [];

  chapters.forEach((chapter, i) => {
    const fname = chapter.path || `chapter-${i + 1}`;
    const title = chapter.title || extractTitle(chapter.content) || fname;
    const body = stripMarkdown(chapter.content);
    const wordCount = body.split(/\s+/).filter(Boolean).length;
    const sentences = splitSentences(body);
    const questions = extractOpenQuestions(sentences, config);
    const lastPara = lastParagraph(body);
    const { endingType, tensionScore, endingSnippet } = detectEnding(lastPara, sentences, config);

    chapterRows.push({
      chapter_index: i,
      file: fname,
      title,
      word_count: wordCount,
      sentence_count: sentences.length,
      question_count: questions.length,
      ending_type: endingType,
      tension_score: tensionScore,
      ending_snippet: endingSnippet,
    });
    chapterTexts.push(body);
  });

  const occurrences = scanEntityOccurrences(chapterTexts);
  const threads = findOpenThreads(occurrences, chapterRows, config);

  const totalChapters = chapterRows.length;
  const totalWords = chapterRows.reduce((s, c) => s + c.word_count, 0);
  const cliffhangerCount = chapterRows.filter(c => c.ending_type === 'cliffhanger').length;
  const resolvedCount = chapterRows.filter(c => c.ending_type === 'resolved').length;
  const totalQuestions = chapterRows.reduce((s, c) => s + c.question_count, 0);
  const avgTension = totalChapters > 0
    ? chapterRows.reduce((s, c) => s + c.tension_score, 0) / totalChapters
    : 0;
  const longestGapChapters = threads.reduce((m, t) => Math.max(m, t.max_gap_chapters), 0);

  const cliffhangerPct = totalChapters > 0 ? (cliffhangerCount / totalChapters) * 100 : 0;
  const chaptersWithQuestions = chapterRows.filter(c => c.question_count > 0).length;
  const questionPct = totalChapters > 0 ? (chaptersWithQuestions / totalChapters) * 100 : 0;
  const threadPct = threads.length === 0
    ? 0
    : Math.min(100, (Math.min(threads.length, totalChapters) / totalChapters) * 100);
  const overallZeigarnikPct = Math.round((cliffhangerPct + questionPct + threadPct) / 3);

  return {
    schema: 'zeigarnik_v1',
    note: 'Textual proxy analysis only — this measures manuscript structure (unresolved endings, open questions, long-gap threads), not reader recall itself. No AI was used to generate these results.',
    summary: {
      overall_zeigarnik_pct: overallZeigarnikPct,
      total_chapters: totalChapters,
      total_words: totalWords,
      cliffhanger_endings: cliffhangerCount,
      resolved_endings: resolvedCount,
      cliffhanger_pct: Math.round(cliffhangerPct),
      total_open_questions: totalQuestions,
      avg_tension_score: Math.round(avgTension * 10) / 10,
      open_thread_count: threads.length,
      longest_gap_chapters: longestGapChapters,
    },
    chapters: chapterRows,
    threads,
  };
}
