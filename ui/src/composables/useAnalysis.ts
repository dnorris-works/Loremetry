import { ref } from 'vue';
import {
  invoke,
  connectJobLogStream,
  disconnectJobLogStream,
  connectAnalysisLogStream,
  disconnectAnalysisLogStream,
  cancelJob,
  waitForJob,
  saveZeigarnikReport,
  saveReadabilityReport,
} from '../api';
import type {
  AnalysisState,
  JobEnqueueResult,
  LogLine,
  SummaryChapterProgress,
  SummaryFileStatus,
} from '../types';
import { listCachedChapters } from '../lib/manuscriptCache';
import { cachedChapterToInput as zeigarnikChapterInput, runZeigarnikAnalysis } from '../lib/zeigarnik';
import { cachedChapterToInput as readabilityChapterInput, runReadabilityAnalysis } from '../lib/readability';

import { useSettings } from './useSettings';
import { refreshAiSpend } from './useAiSpend';

const analysisState = ref<AnalysisState | null>(null);
const isWorking = ref(false);
const logLines = ref<LogLine[]>([]);
const summaryFileProgress = ref<Record<string, SummaryFileStatus>>({});
let currentJobId = '';

function resetSummaryFileProgress(files: string[]): void {
  const next: Record<string, SummaryFileStatus> = {};
  for (const file of files) {
    next[file] = 'pending';
  }
  summaryFileProgress.value = next;
}

function beginSummaryTracking(): void {
  const s = analysisState.value;
  if (!s) {
    summaryFileProgress.value = {};
    return;
  }
  resetSummaryFileProgress([
    ...(s.summary_missing_files || []),
    ...(s.summary_stale_files || []),
  ]);
}

function applySummaryProgress(payload: SummaryChapterProgress): void {
  const { filename, status } = payload;
  if (!filename) return;
  if (status === 'started') {
    summaryFileProgress.value = { ...summaryFileProgress.value, [filename]: 'active' };
    return;
  }
  if (status === 'done' || status === 'skipped') {
    summaryFileProgress.value = { ...summaryFileProgress.value, [filename]: status };
  }
}

function classifyLogLine(msg: string): LogLine {
  const trimmed = msg.trimStart();

  if (trimmed.startsWith('✓')) {
    return { type: 'log-success', icon: '✓', text: trimmed.slice(1).trim() };
  }
  if (trimmed.startsWith('✗')) {
    return { type: 'log-error', icon: '✗', text: trimmed.slice(1).trim() };
  }
  if (trimmed.startsWith('⚠')) {
    return { type: 'log-warn', icon: '⚠', text: trimmed.slice(1).trim() };
  }
  if (trimmed.startsWith('→')) {
    return { type: 'log-item', icon: '→', text: trimmed.slice(1).trim() };
  }
  if (/^(Step \d|Phase \d|Running |Analyzing |Mining |Syncing )/i.test(trimmed)) {
    return { type: 'log-step', icon: '', text: trimmed };
  }
  if (/complete\.?$|complete —|done\.?$/i.test(trimmed)) {
    return { type: 'log-done', icon: '', text: trimmed };
  }
  if (msg.startsWith('    ') || msg.startsWith('\t\t')) {
    return { type: 'log-detail', icon: '', text: trimmed };
  }
  return { type: 'log-info', icon: '', text: trimmed };
}

function appendLog(msg: string): void {
  logLines.value.push(classifyLogLine(msg));
}

function clearLog(): void {
  logLines.value = [];
}

async function refreshState(folder: string): Promise<void> {
  if (!folder) {
    analysisState.value = null;
    return;
  }
  try {
    const state = await invoke<AnalysisState>('check_analysis_state', { folder });
    analysisState.value = state;
  } catch (e) {
    console.error('check_analysis_state:', e);
    analysisState.value = null;
  }
}

function getSettings() {
  const s = useSettings();
  return {
    provider: s.provider.value,
    model: s.model.value,
  };
}

async function finishQueuedJob(jobId: string): Promise<void> {
  currentJobId = jobId;
  connectJobLogStream(jobId, appendLog, applySummaryProgress);
  try {
    const job = await waitForJob(jobId);
    if (job.status === 'completed' && job.result && !job.result.success) {
      appendLog('✗ ' + job.result.error);
    } else if (job.status === 'failed' || job.status === 'cancelled') {
      appendLog('✗ ' + (job.error || 'Job failed'));
    }
  } finally {
    disconnectJobLogStream();
    currentJobId = '';
  }
}

async function runAnalyze(
  folder: string,
  forceResummarize: boolean,
  platform: string,
  selected: string[] = [],
  formats: { publishEbook: boolean; publishPrint: boolean } = { publishEbook: true, publishPrint: true },
): Promise<void> {
  if (!folder) { appendLog('✗ No story selected.'); return; }
  const s = useSettings();
  const { provider, model } = getSettings();

  clearLog();
  isWorking.value = true;
  beginSummaryTracking();
  const runTime = new Date().toISOString();

  try {
    const queued = await invoke<JobEnqueueResult>('analyze_story', {
      request: {
        folder, model, provider,
        force_resummarize: forceResummarize,
        platform,
        run_time: runTime,
        selected,
        publish_ebook: formats.publishEbook,
        publish_print: formats.publishPrint,
        genre_model: s.modelFor('genre'),
        summaries_model: s.modelFor('summaries'),
      },
    });
    await finishQueuedJob(queued.job_id);
  } catch (e) {
    appendLog('✗ ' + String(e));
  } finally {
    isWorking.value = false;
    void refreshAiSpend();
    await refreshState(folder);
    saveLog(folder, runTime);
  }
}

export type ContinuityScope = { mode: 'manuscript' } | { mode: 'series'; seriesId: number };

/**
 * Runs the craft pipeline via a single backend command.
 * The backend handles ordering, AI calls, and storage.
 */
async function runCraftAnalysis(
  folder: string,
  selected: string[],
  continuityScope: ContinuityScope,
  seriesIdForAudits?: number,
): Promise<void> {
  if (!folder) { appendLog('✗ No story selected.'); return; }

  const s = useSettings();
  const { provider } = getSettings();
  clearLog();
  isWorking.value = true;
  beginSummaryTracking();

  let serverSelected = [...selected];
  const resolvedSeriesId = continuityScope.mode === 'series'
    ? continuityScope.seriesId
    : (seriesIdForAudits ?? 0);

  try {
    const cached = await listCachedChapters(folder);

    if (selected.includes('zeigarnik_analysis') && cached.length > 0) {
      appendLog(`Found ${cached.length} chapter(s). Scanning for open loops locally (no AI — pattern matching only)...`);
      try {
        const inputs = cached.map(zeigarnikChapterInput);
        const report = runZeigarnikAnalysis(inputs);
        await saveZeigarnikReport(folder, JSON.stringify(report));
        appendLog('✓ Zeigarnik analysis saved to database.');
        serverSelected = serverSelected.filter(id => id !== 'zeigarnik_analysis');
      } catch (e) {
        appendLog(`⚠ Local Zeigarnik failed (${String(e)}); falling back to server.`);
      }
    }

    if (selected.includes('readability_analysis') && cached.length > 0) {
      appendLog(`Analyzing readability for ${cached.length} chapter(s) locally (formula-based — no AI)...`);
      try {
        const inputs = cached.map(readabilityChapterInput);
        const report = runReadabilityAnalysis(inputs);
        await saveReadabilityReport(folder, JSON.stringify(report));
        appendLog('✓ Readability report saved to database.');
        serverSelected = serverSelected.filter(id => id !== 'readability_analysis');
      } catch (e) {
        appendLog(`✗ Readability analysis failed: ${String(e)}`);
      }
    } else if (selected.includes('readability_analysis') && cached.length === 0) {
      appendLog('✗ Readability needs chapter text in this browser. Upload chapters in Sources first.');
      serverSelected = serverSelected.filter(id => id !== 'readability_analysis');
    }

    if (serverSelected.length === 0) {
      return;
    }

    const queued = await invoke<JobEnqueueResult>('run_craft_pipeline', {
      request: {
        folder,
        selected: serverSelected,
        provider,
        model: s.modelFor('default'),
        model_summaries: s.modelFor('summaries'),
        model_continuity: s.modelFor('continuity'),
        model_sdt: s.modelFor('showDontTell'),
        model_ai_isms: s.modelFor('aiIsms'),
        model_prose: s.modelFor('prose'),
        continuity_scope: continuityScope.mode,
        series_id: resolvedSeriesId,
      },
    });
    await finishQueuedJob(queued.job_id);
  } catch (e) {
    appendLog('✗ ' + String(e));
  } finally {
    isWorking.value = false;
    void refreshAiSpend();
    await refreshState(folder);
    saveLog(folder, new Date().toISOString());
  }
}

async function runMarketIntel(folder: string): Promise<void> {
  if (!folder) { appendLog('✗ No story selected.'); return; }
  const { provider, model } = getSettings();

  clearLog();
  isWorking.value = true;

  try {
    const queued = await invoke<JobEnqueueResult>('run_market_intel', {
      request: { folder, provider, model },
    });
    await finishQueuedJob(queued.job_id);
  } catch (e) {
    appendLog('✗ ' + String(e));
  } finally {
    isWorking.value = false;
    void refreshAiSpend();
    saveLog(folder, new Date().toISOString());
  }
}

async function runSummaries(folder: string): Promise<void> {
  if (!folder) { appendLog('✗ No story selected.'); return; }
  const s = useSettings();
  const setupIssues = s.checkPublishAnalyzeSetup();
  if (setupIssues.length > 0) {
    appendLog('✗ Setup required before summarizing:\n' + setupIssues.map(i => `• ${i.message}`).join('\n'));
    return;
  }

  clearLog();
  isWorking.value = true;
  beginSummaryTracking();
  hasRunHint();
  const runTime = new Date().toISOString();

  connectAnalysisLogStream(appendLog, applySummaryProgress);
  try {
    const report = await invoke<string>('refresh_chapter_summaries', {
      request: {
        folder,
        provider: s.provider.value,
        api_key: '',
        model: s.modelFor('summaries') || s.model.value,
      },
    });
    appendLog(report || '✓ Chapter summaries refreshed.');
  } catch (e) {
    appendLog('✗ ' + String(e));
  } finally {
    disconnectAnalysisLogStream();
    isWorking.value = false;
    void refreshAiSpend();
    await refreshState(folder);
    saveLog(folder, runTime);
  }
}

/** Soft signal so Analyzer can show LogStream during summary-only runs. */
const summaryRunActive = ref(false);
function hasRunHint(): void {
  summaryRunActive.value = true;
}

async function cancelOperation(): Promise<void> {
  appendLog('Stopping after current step...');
  if (currentJobId) {
    try {
      await cancelJob(currentJobId);
    } catch (e) {
      appendLog('✗ ' + String(e));
    }
  } else {
    await invoke('cancel_operation');
  }
}

async function saveLog(folder: string, timestamp: string): Promise<void> {
  if (!folder || logLines.value.length === 0) return;
  const content = JSON.stringify({
    schema: 'activity_log_v1',
    timestamp,
    lines: logLines.value,
  });
  try {
    await invoke('save_activity_log_cmd', { folder, content, timestamp });
  } catch (e) {
    console.error('Failed to save activity log:', e);
  }
}

export function useAnalysis() {
  return {
    analysisState,
    isWorking,
    logLines,
    summaryFileProgress,
    summaryRunActive,
    refreshState,
    runAnalyze,
    runCraftAnalysis,
    runMarketIntel,
    runSummaries,
    cancelOperation,
    clearLog,
    appendLog,
  };
}
