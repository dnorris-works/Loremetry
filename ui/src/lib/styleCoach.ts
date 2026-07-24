/**
 * Live prose style coaching — deterministic rules, no AI.
 * Highlights adverbs, passive voice, and long sentences while you write.
 */

import type { Node as PMNode } from '@tiptap/pm/model';
import { analyzeTextReadability, countSyllables, splitSentences, tokenizeWords } from './readability';

export type StyleCoachIssueType = 'adverb' | 'passive' | 'hard' | 'very-hard' | 'complex';

export interface StyleCoachHighlight {
  type: StyleCoachIssueType;
  from: number;
  to: number;
  text: string;
}

export interface StyleCoachStats {
  wordCount: number;
  sentenceCount: number;
  paragraphCount: number;
  adverbCount: number;
  passiveCount: number;
  hardSentenceCount: number;
  veryHardSentenceCount: number;
  complexWordCount: number;
  gradeLevel: number;
  readingEase: number;
  readingTimeMinutes: number;
}

const HARD_SENTENCE_WORDS = 14;
const VERY_HARD_SENTENCE_WORDS = 21;
const WORDS_PER_MINUTE = 200;

const ADVERB_EXCEPTIONS = new Set([
  'only', 'early', 'daily', 'weekly', 'monthly', 'yearly', 'hourly',
  'family', 'italy', 'ugly', 'holy', 'july', 'supply', 'apply', 'reply',
  'multi', 'ali', 'balcony', 'monopoly', 'anomaly',
]);

const PASSIVE_AUX = '(?:am|is|are|was|were|be|been|being|get|gets|got|gotten|become|becomes|became)';
const PASSIVE_PARTICIPLE = String.raw`(?:\w+ed|built|bought|caught|chosen|done|drawn|driven|eaten|fallen|forgiven|frozen|given|gone|grown|heard|held|hidden|hit|kept|known|laid|led|left|lost|made|meant|met|paid|put|read|rid|ridden|rung|risen|run|said|seen|sent|set|shaken|shown|shut|slain|slept|sold|spent|split|spread|sprung|stood|stolen|stuck|struck|sung|sunk|swum|sworn|taken|taught|thought|thrown|told|torn|worn|won|wound|written)`;
const PASSIVE_RE = new RegExp(`\\b${PASSIVE_AUX}\\s+${PASSIVE_PARTICIPLE}\\b`, 'gi');

function findAllMatches(text: string, re: RegExp): { from: number; to: number; text: string }[] {
  const out: { from: number; to: number; text: string }[] = [];
  const flags = re.flags;
  const globalRe = re.global ? re : new RegExp(re.source, flags.includes('g') ? flags : `${flags}g`);
  for (const m of text.matchAll(globalRe)) {
    if (m.index == null) continue;
    out.push({ from: m.index, to: m.index + m[0].length, text: m[0] });
  }
  return out;
}

function findAdverbs(text: string): StyleCoachHighlight[] {
  const re = /\b[a-z]+ly\b/gi;
  const out: StyleCoachHighlight[] = [];
  for (const m of text.matchAll(re)) {
    if (m.index == null) continue;
    const word = m[0].toLowerCase();
    if (ADVERB_EXCEPTIONS.has(word)) continue;
    out.push({ type: 'adverb', from: m.index, to: m.index + m[0].length, text: m[0] });
  }
  return out;
}

function findPassiveVoice(text: string): StyleCoachHighlight[] {
  return findAllMatches(text, PASSIVE_RE).map(m => ({
    type: 'passive' as const,
    ...m,
  }));
}

function sentenceRanges(text: string): { from: number; to: number; text: string; wordCount: number }[] {
  const ranges: { from: number; to: number; text: string; wordCount: number }[] = [];
  const re = /[^.!?]*[.!?]+[\)\]\u201d'\u2019\u201c\u0022]*/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const s = match[0].trim();
    if (!s) continue;
    ranges.push({
      from: match.index,
      to: match.index + match[0].length,
      text: s,
      wordCount: tokenizeWords(s).length,
    });
  }
  const matchedEnd = ranges.reduce((max, r) => Math.max(max, r.to), 0);
  const tail = text.slice(matchedEnd).trim();
  if (tail && tokenizeWords(tail).length >= 3) {
    ranges.push({
      from: matchedEnd,
      to: text.length,
      text: tail,
      wordCount: tokenizeWords(tail).length,
    });
  }
  if (ranges.length === 0 && text.trim()) {
    ranges.push({
      from: 0,
      to: text.length,
      text: text.trim(),
      wordCount: tokenizeWords(text).length,
    });
  }
  return ranges;
}

function findLongSentences(text: string): StyleCoachHighlight[] {
  const out: StyleCoachHighlight[] = [];
  for (const s of sentenceRanges(text)) {
    if (s.wordCount >= VERY_HARD_SENTENCE_WORDS) {
      out.push({ type: 'very-hard', from: s.from, to: s.to, text: s.text });
    } else if (s.wordCount >= HARD_SENTENCE_WORDS) {
      out.push({ type: 'hard', from: s.from, to: s.to, text: s.text });
    }
  }
  return out;
}

function findComplexWords(text: string): StyleCoachHighlight[] {
  const re = /\b[A-Za-z']+(?:'[A-Za-z]+)?\b/g;
  const out: StyleCoachHighlight[] = [];
  for (const m of text.matchAll(re)) {
    if (m.index == null) continue;
    const word = m[0];
    if (countSyllables(word) >= 3 && word.length >= 5) {
      out.push({ type: 'complex', from: m.index, to: m.index + word.length, text: word });
    }
  }
  return out;
}

export function analyzeStyleCoach(text: string): { stats: StyleCoachStats; highlights: StyleCoachHighlight[] } {
  const plain = text.trim();
  const readability = analyzeTextReadability(plain);
  const sentences = splitSentences(plain);
  const paragraphs = plain.split(/\n\s*\n/).filter(p => p.trim()).length || (plain ? 1 : 0);

  const adverbs = findAdverbs(plain);
  const passive = findPassiveVoice(plain);
  const longSentences = findLongSentences(plain);
  const complex = findComplexWords(plain);

  const hardCount = longSentences.filter(h => h.type === 'hard').length;
  const veryHardCount = longSentences.filter(h => h.type === 'very-hard').length;

  return {
    stats: {
      wordCount: readability.wordCount,
      sentenceCount: sentences.length,
      paragraphCount: paragraphs,
      adverbCount: adverbs.length,
      passiveCount: passive.length,
      hardSentenceCount: hardCount,
      veryHardSentenceCount: veryHardCount,
      complexWordCount: complex.length,
      gradeLevel: readability.fleschKincaidGrade,
      readingEase: readability.fleschReadingEase,
      readingTimeMinutes: readability.wordCount > 0
        ? Math.max(1, Math.round(readability.wordCount / WORDS_PER_MINUTE))
        : 0,
    },
    highlights: [
      ...longSentences,
      ...passive,
      ...adverbs,
      ...complex,
    ],
  };
}

/** Map plain-text offsets (doc.textContent) to ProseMirror document positions. */
export function offsetsToDocRange(
  doc: PMNode,
  from: number,
  to: number,
): { from: number; to: number } | null {
  if (from >= to) return null;

  const chunks: { pos: number; start: number; end: number }[] = [];
  let offset = 0;
  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;
    const len = node.text.length;
    chunks.push({ pos, start: offset, end: offset + len });
    offset += len;
  });

  if (offset === 0) return null;

  let pmFrom: number | null = null;
  let pmTo: number | null = null;

  for (const c of chunks) {
    if (pmFrom === null && from >= c.start && from < c.end) {
      pmFrom = c.pos + (from - c.start);
    }
    if (to > c.start && to <= c.end) {
      pmTo = c.pos + (to - c.start);
      break;
    }
    if (to > c.end && from < c.end) {
      pmTo = c.pos + (c.end - c.start);
    }
  }

  if (pmFrom === null || pmTo === null || pmTo <= pmFrom) return null;
  return { from: pmFrom, to: pmTo };
}

/** Resolve a highlight to document positions; falls back to phrase search. */
export function highlightToDocRange(
  doc: PMNode,
  highlight: StyleCoachHighlight,
): { from: number; to: number } | null {
  const direct = offsetsToDocRange(doc, highlight.from, highlight.to);
  if (direct) return direct;

  const text = highlight.text?.trim();
  if (!text) return null;

  const full = doc.textContent;
  const start = Math.max(0, highlight.from - 20);
  const idx = full.indexOf(text, start);
  if (idx < 0) {
    const loose = full.indexOf(text);
    if (loose < 0) return null;
    return offsetsToDocRange(doc, loose, loose + text.length);
  }
  return offsetsToDocRange(doc, idx, idx + text.length);
}
