<script setup lang="ts">
import { inject, ref, computed, watch, onMounted } from 'vue';
import { invoke } from '../api';
import type { ContinuityScope } from '../composables/useAnalysis';
import { useSettings } from '../composables/useSettings';
import { storiesKey, analysisKey, seriesKey, platformKey, showPanelKey } from '../injectionKeys';
import LogStream from './LogStream.vue';
import AnalyzerPlatformTabs from './AnalyzerPlatformTabs.vue';
import { useReportTypes } from '../composables/useReportTypes';
import { useCraftReportGroups } from '../composables/useCraftReportGroups';
import { getChapterWordStats } from '../lib/manuscriptCache';
import { estimateReportCosts } from '../lib/estimateCosts';
import { buildRunQueue, collectPrerequisites } from '../reportDependencies';
import { isAiConfigured, resolveModelPrices } from '../lib/reportCostPricing';
import type { ReportTypeDef, Series } from '../types';
import { useAuth } from '../composables/useAuth';
import { reportAccessBadge, reportAccessLabel, isSubscriberRole } from '../lib/reportAccess';

type VisibleReport = ReportTypeDef & {
  exists: boolean;
  freshness: 'fresh' | 'stale' | 'missing';
};

type DepRow = {
  id: string;
  label: string;
  freshness: 'fresh' | 'stale' | 'missing';
};

type ReportSection = {
  id: string;
  label: string;
  subtitle: string;
  reports: VisibleReport[];
  showHeader: boolean;
  disabled: boolean;
  disabledReason: string;
};

// ── Injections ────────────────────────────────────────────────────────────────

const storiesCtx = inject(storiesKey)!;
const analysisCtx = inject(analysisKey)!;
const seriesCtx = inject(seriesKey)!;
const platformCtx = inject(platformKey)!;
const showPanel = inject(showPanelKey);
const settings = useSettings();
const auth = useAuth();

const isSubscriber = computed(() =>
  isSubscriberRole(auth.me.value?.role ?? '', auth.isAdmin.value),
);

function tierForReport(report: VisibleReport) {
  return reportAccessBadge(report.min_tier, isSubscriber.value);
}

// ── Report types from DB ──────────────────────────────────────────────────────

const { reportTypes, loadError, loaded: reportTypesLoaded } = useReportTypes();
const { craftReportGroups, seriesReportIds, loadCraftReportGroups } = useCraftReportGroups();

onMounted(() => {
  loadCraftReportGroups();
  fetchCostEstimates();
});

const activeStorySeries = computed((): Series | null => {
  const folder = storiesCtx.activeFolder.value;
  if (!folder) return null;
  return seriesCtx.series.value.find(s =>
    s.books.some(b => b.story_id === folder),
  ) ?? null;
});

// ── Local state ───────────────────────────────────────────────────────────────

const selected = ref<string[]>([]);
const depRunOverrides = ref<Record<string, boolean>>({});
const forceResummarize = ref(false);
const publishEbook = ref(true);
const publishPrint = ref(true);
const hasRun = ref(false);
const continuityScopeMode = ref<'manuscript' | 'series'>('manuscript');
const continuitySeriesId = ref<number | null>(null);

// ── Computed ──────────────────────────────────────────────────────────────────

const freshnessMap = computed(() => {
  const s = analysisCtx.analysisState.value;
  const map: Record<string, 'fresh' | 'stale' | 'missing'> = {};
  if (!s?.report_freshness) return map;
  for (const r of s.report_freshness) {
    map[r.doc_type] = r.status;
  }
  return map;
});

const existsMap = computed(() => {
  const state = analysisCtx.analysisState.value;
  if (!state) return {} as Record<string, boolean>;
  const docs = new Set(state.existing_docs || []);
  const map: Record<string, boolean> = {
    chapter_summaries: state.summary_count > 0,
    genre_analysis: state.has_genre_data,
    genre_ranking: state.has_genre_ranking,
    kdp_categories: state.has_categories,
    kdp_keywords: state.has_keywords,
    bisac_classification: state.has_bisac,
    mi_search_terms: state.has_search_terms,
    discovery_keywords: state.has_discovery_keywords,
    analysis: state.has_full_report,
    wide_analysis: state.has_wide_analysis,
    keyword_search: state.has_keyword_search_results,
    competition_report: state.has_competition,
    review_mining: docs.has('review_mining'),
    author_analysis: docs.has('author_analysis'),
    activity_log: docs.has('activity_log'),
    zeigarnik_analysis: state.has_zeigarnik,
    readability_analysis: state.has_readability,
    continuity_check: state.has_continuity_check,
    show_dont_tell: state.has_show_dont_tell,
    ai_isms: state.has_ai_isms,
  };
  for (const id of docs) {
    if (!(id in map)) map[id] = true;
  }
  return map;
});

function getReportFreshness(reportId: string): 'fresh' | 'stale' | 'missing' {
  if (reportId === 'chapter_summaries') {
    const s = analysisCtx.analysisState.value;
    if (!s || !storiesCtx.activeFolder.value) return 'missing';
    if (s.summary_chapter_count === 0) return 'missing';
    if (s.summary_missing_count > 0) return 'missing';
    if (s.summary_stale_count > 0) return 'stale';
    return 'fresh';
  }
  return freshnessMap.value[reportId]
    ?? (existsMap.value[reportId] ? 'stale' : 'missing');
}

const visibleReports = computed((): VisibleReport[] => {
  const plat = platformCtx.platform.value;
  return reportTypes.value
    .filter(r => r.platforms.includes(plat) && r.id !== 'chapter_summaries')
    .map(r => ({
      ...r,
      exists: existsMap.value[r.id] ?? false,
      freshness: getReportFreshness(r.id),
    }));
});

const summaryStatus = computed(() => {
  const s = analysisCtx.analysisState.value;
  if (!s || !storiesCtx.activeFolder.value) {
    return { needsRefresh: false, text: 'Select a story to manage chapter summaries.' };
  }
  if (s.summary_chapter_count === 0) {
    return { needsRefresh: false, text: 'No manuscript chapters found yet.' };
  }
  if (s.summary_missing_count > 0 || s.summary_stale_count > 0) {
    const parts: string[] = [];
    if (s.summary_missing_count > 0) parts.push(`${s.summary_missing_count} new/unscanned`);
    if (s.summary_stale_count > 0) parts.push(`${s.summary_stale_count} changed since last scan`);
    return {
      needsRefresh: true,
      text: `Chapter summaries need refresh: ${parts.join(', ')}.`,
    };
  }
  return {
    needsRefresh: false,
    text: `Chapter summaries are up to date (${s.summary_count}/${s.summary_chapter_count}). Manage in Settings → Story Data.`,
  };
});

const summaryIssueFiles = computed(() => {
  const s = analysisCtx.analysisState.value;
  if (!s) {
    return { missing: [] as string[], stale: [] as string[] };
  }
  return {
    missing: s.summary_missing_files || [],
    stale: s.summary_stale_files || [],
  };
});

function sectionAvailability(groupId: string): { disabled: boolean; reason: string } {
  if (groupId === 'series') {
    if (!storiesCtx.activeFolder.value) {
      return { disabled: true, reason: 'Select a story first.' };
    }
    if (!activeStorySeries.value) {
      return { disabled: true, reason: 'Add this story to a series in the Series panel.' };
    }
  }
  return { disabled: false, reason: '' };
}

const reportSections = computed((): ReportSection[] => {
  const reports = visibleReports.value;
  if (platformCtx.platform.value !== 'craft') {
    return [{
      id: 'all',
      label: '',
      subtitle: '',
      reports,
      showHeader: false,
      disabled: false,
      disabledReason: '',
    }];
  }

  const byId = new Map(reports.map(r => [r.id, r]));
  return craftReportGroups.value
    .map(group => {
      const availability = sectionAvailability(group.id);
      return {
        id: group.id,
        label: group.label,
        subtitle: availability.disabled ? availability.reason : group.subtitle,
        reports: group.reportIds
          .map(id => byId.get(id))
          .filter((r): r is VisibleReport => r != null),
        showHeader: true,
        disabled: availability.disabled,
        disabledReason: availability.reason,
      };
    })
    .filter(group => group.reports.length > 0);
});

const canSelectReports = computed(() => Boolean(storiesCtx.activeFolder.value));

const reportsLocked = computed(() => analysisCtx.isWorking.value || !canSelectReports.value);

const setupIssues = computed(() => {
  const plat = platformCtx.platform.value;
  if (plat === 'craft' || plat === 'publish') return settings.checkCraftAnalyzeSetup();
  return settings.checkPublishAnalyzeSetup();
});

const marketIntelSetupIssues = computed(() => settings.checkMarketIntelSetup());

function openSettings(): void {
  showPanel?.('settings');
}

const getReportsDisabled = computed(() => {
  return reportsLocked.value || selected.value.length === 0 || setupIssues.value.length > 0;
});

// ── Checkbox logic ────────────────────────────────────────────────────────────

function defaultDepRuns(depId: string): boolean {
  return getReportFreshness(depId) !== 'fresh';
}

function isDepInRunQueue(depId: string): boolean {
  if (depId in depRunOverrides.value) {
    return depRunOverrides.value[depId];
  }
  return defaultDepRuns(depId);
}

function setDepRunOverride(depId: string, run: boolean): void {
  depRunOverrides.value = { ...depRunOverrides.value, [depId]: run };
}

function onDepCheckboxChange(depId: string, event: Event): void {
  const target = event.target;
  if (target instanceof HTMLInputElement) {
    setDepRunOverride(depId, target.checked);
  }
}

function prerequisitesForReport(reportId: string): DepRow[] {
  return collectPrerequisites(reportId, reportTypes.value).map(id => ({
    id,
    label: reportTypes.value.find(r => r.id === id)?.label ?? id,
    freshness: getReportFreshness(id),
  }));
}

function toggleReport(id: string, disabled = false): void {
  if (disabled || reportsLocked.value) return;
  const sel = new Set(selected.value);

  if (sel.has(id)) {
    sel.delete(id);
  } else {
    sel.add(id);
  }

  selected.value = [...sel];
}

function groupSelectionState(reports: VisibleReport[]): 'all' | 'some' | 'none' {
  const ids = reports.map(r => r.id);
  const count = ids.filter(id => selected.value.includes(id)).length;
  if (count === 0) return 'none';
  if (count === ids.length) return 'all';
  return 'some';
}

function toggleGroupSelection(reports: VisibleReport[], disabled = false): void {
  if (disabled || reportsLocked.value) return;
  const ids = reports.map(r => r.id);
  const sel = new Set(selected.value);
  const selectAll = groupSelectionState(reports) !== 'all';

  for (const id of ids) {
    if (selectAll) {
      sel.add(id);
    } else {
      sel.delete(id);
    }
  }

  selected.value = [...sel];
}

// Reset selection when platform or story changes
watch(() => platformCtx.platform.value, () => {
  selected.value = [];
  depRunOverrides.value = {};
});

watch(() => storiesCtx.activeFolder.value, (folder) => {
  depRunOverrides.value = {};
  if (!folder) {
    selected.value = [];
  }
});

// ── Cost estimation ───────────────────────────────────────────────────────────

const costEstimates = ref<Record<string, number | null>>({});
const costEstimatesLoaded = ref(false);

const aiConfigured = computed(() =>
  isAiConfigured('ok', settings.model.value),
);

const reportsToRun = computed(() => {
  const plat = platformCtx.platform.value;
  const primaries = selected.value.filter(id => {
    const def = reportTypes.value.find(r => r.id === id);
    return def?.platforms.includes(plat);
  });
  return buildRunQueue(primaries, reportTypes.value, isDepInRunQueue);
});

function isAiReport(reportId: string): boolean {
  const rt = reportTypes.value.find(r => r.id === reportId);
  if (!rt) return true;
  return rt.cost_output_max > 0 || rt.cost_per_chapter || rt.cost_fixed_calls > 0;
}

function hasSummaryDependency(reportId: string, visited = new Set<string>()): boolean {
  if (reportId === 'chapter_summaries') return true;
  if (visited.has(reportId)) return false;
  visited.add(reportId);

  const report = reportTypes.value.find(r => r.id === reportId);
  if (!report) return false;
  return report.depends_on.some(dep => hasSummaryDependency(dep, visited));
}

function selectionNeedsSummaries(): boolean {
  return reportsToRun.value.includes('chapter_summaries')
    || reportsToRun.value.some(id => hasSummaryDependency(id));
}

function pricingForReport(reportId: string): ReturnType<typeof resolveModelPrices> {
  const modelId = settings.modelFor(reportToModelFn(reportId));
  return resolveModelPrices(modelId, settings.models.value);
}

function depStatusLabel(freshness: 'fresh' | 'stale' | 'missing'): string {
  if (freshness === 'fresh') return 'has run';
  if (freshness === 'stale') return 'stale — re-run recommended';
  return 'not run yet';
}

async function maybeRefreshSummariesBeforeRun(folder: string): Promise<boolean> {
  if (!selectionNeedsSummaries() || !summaryStatus.value.needsRefresh) {
    return true;
  }

  const summaryPricing = pricingForReport('chapter_summaries');

  let msg = 'Some chapters need AI summarization before these reports can run.\n\n';
  try {
    const estimate = await invoke<{
      success: boolean;
      chapter_count: number;
      input_tokens: number;
      output_tokens: number;
      estimated_cost: number | null;
      error: string;
    }>('estimate_summary_refresh_cost', {
      request: {
        folder,
        input_price: summaryPricing.available ? summaryPricing.input_price : undefined,
        output_price: summaryPricing.available ? summaryPricing.output_price : undefined,
      },
    });
    const count = estimate.success ? estimate.chapter_count : 0;
    if (count > 0) {
      msg += `Chapters to summarize: ${count}\n`;
      if (estimate.input_tokens > 0) {
        msg += `Estimated tokens: ~${estimate.input_tokens.toLocaleString()} in / ~${estimate.output_tokens.toLocaleString()} out\n`;
      }
      if (estimate.estimated_cost != null && summaryPricing.available) {
        msg += `Estimated cost: ${formatCost(estimate.estimated_cost)}\n`;
      } else if (!summaryPricing.available) {
        msg += 'Estimated cost: pricing unavailable — fetch models in Settings → AI Models\n';
      }
    } else {
      msg += `${summaryStatus.value.text}\n`;
    }
  } catch {
    msg += `${summaryStatus.value.text}\n`;
  }
  msg += '\nSummarize chapters now?';

  if (!confirm(msg)) return false;

  await analysisCtx.runSummaries(folder);

  const s = analysisCtx.analysisState.value;
  if (s && (s.summary_missing_count > 0 || s.summary_stale_count > 0)) {
    alert('Chapter summaries are still not up to date after refresh. Please resolve chapter read errors and try again.');
    return false;
  }
  return true;
}

const reportsMissingPricing = computed(() =>
  reportsToRun.value.filter(id =>
    isAiReport(id) && costEstimatesLoaded.value && costEstimates.value[id] == null,
  ),
);

const totalEstimatedCost = computed(() => {
  let total = 0;
  for (const id of reportsToRun.value) {
    const est = costEstimates.value[id];
    if (est != null) total += est;
  }
  return total;
});

function formatCost(cost: number): string {
  if (cost === 0) return '$0.00';
  if (cost < 0.01) return '<$0.01';
  return `~$${cost.toFixed(2)}`;
}

function formatTotalCost(): string {
  if (!aiConfigured.value) return '';
  if (!costEstimatesLoaded.value) return '…';
  const missing = reportsMissingPricing.value.length;
  if (reportsToRun.value.length === 0) return '';
  const aiReports = reportsToRun.value.filter(id => isAiReport(id));
  if (missing === aiReports.length && aiReports.length > 0) {
    return 'pricing unavailable';
  }
  const base = formatCost(totalEstimatedCost.value);
  if (missing > 0) return `${base} (${missing} unpriced)`;
  return base;
}

function reportRunCost(reportId: string, usesAi = true): string {
  if (!usesAi) return 'Free';
  if (!aiConfigured.value) return '—';
  if (!costEstimatesLoaded.value) return '…';
  const estimate = costEstimates.value[reportId];
  if (estimate == null) return 'pricing unavailable';
  return formatCost(estimate);
}

async function fetchCostEstimates(): Promise<void> {
  const folder = storiesCtx.activeFolder.value;
  costEstimatesLoaded.value = false;
  if (!folder || visibleReports.value.length === 0 || settings.models.value.length === 0) {
    costEstimates.value = {};
    costEstimatesLoaded.value = true;
    return;
  }

  const modelPrices = visibleReports.value.map(r => {
    const fnKey = reportToModelFn(r.id);
    const modelId = settings.modelFor(fnKey);
    const pricing = resolveModelPrices(modelId, settings.models.value);
    return {
      report_id: r.id,
      input_price: pricing.available ? pricing.input_price : -1,
      output_price: pricing.available ? pricing.output_price : -1,
    };
  });

  try {
    const stats = await getChapterWordStats(folder);
    if (stats.chapterCount > 0) {
      const pricedOnly = modelPrices.filter(p => p.input_price >= 0);
      const estimates = estimateReportCosts(visibleReports.value, pricedOnly, stats);
      const obj: Record<string, number | null> = {};
      for (const r of visibleReports.value) {
        const pricing = resolveModelPrices(
          settings.modelFor(reportToModelFn(r.id)),
          settings.models.value,
        );
        if (!pricing.available) {
          obj[r.id] = null;
          continue;
        }
        const est = estimates.find(e => e.report_id === r.id);
        obj[r.id] = est?.estimated_cost ?? 0;
      }
      costEstimates.value = obj;
      costEstimatesLoaded.value = true;
      return;
    }

    const result = await invoke<{ success: boolean; estimates: { report_id: string; estimated_cost: number }[] }>('estimate_report_costs', {
      request: { folder, model_prices: modelPrices.filter(p => p.input_price >= 0) },
    });
    const obj: Record<string, number | null> = {};
    for (const r of visibleReports.value) {
      const pricing = resolveModelPrices(
        settings.modelFor(reportToModelFn(r.id)),
        settings.models.value,
      );
      if (!pricing.available) {
        obj[r.id] = null;
        continue;
      }
      const est = result.success
        ? result.estimates.find(e => e.report_id === r.id)
        : undefined;
      obj[r.id] = est?.estimated_cost ?? 0;
    }
    costEstimates.value = obj;
  } catch (e) {
    console.error('estimate_report_costs:', e);
  } finally {
    costEstimatesLoaded.value = true;
  }
}

/** Map report_id to the modelFor() function key (from report_types.model_slot). */
function reportToModelFn(reportId: string): 'default' | 'summaries' | 'genre' | 'keywords' | 'continuity' | 'showDontTell' | 'aiIsms' | 'prose' {
  const slot = reportTypes.value.find(r => r.id === reportId)?.model_slot;
  switch (slot) {
    case 'summaries':
    case 'genre':
    case 'keywords':
    case 'continuity':
    case 'showDontTell':
    case 'aiIsms':
    case 'prose':
      return slot;
    default:
      return 'default';
  }
}

// Refresh estimates when folder changes, models are loaded, or report types load
watch(() => storiesCtx.activeFolder.value, () => fetchCostEstimates(), { flush: 'post' });
watch(() => settings.models.value, () => fetchCostEstimates());
watch(() => reportTypes.value, () => fetchCostEstimates());

// ── Handlers ──────────────────────────────────────────────────────────────────

async function onGetReports(): Promise<void> {
  const folder = storiesCtx.activeFolder.value;
  if (!folder) return;
  if (setupIssues.value.length > 0) {
    alert(setupIssues.value.map(i => i.message).join('\n'));
    return;
  }

  const ready = await maybeRefreshSummariesBeforeRun(folder);
  if (!ready) return;

  hasRun.value = true;
  const plat = platformCtx.platform.value;
  const toRun = reportsToRun.value.filter(id => {
    const def = reportTypes.value.find(r => r.id === id);
    return def?.platforms.includes(plat);
  });
  if (toRun.length === 0) return;

  if (plat === 'craft' || plat === 'publish') {
    const hasSeriesReports = toRun.some(id => seriesReportIds.value.includes(id));
    const continuityInSeriesMode = toRun.includes('continuity_check')
      && continuityScopeMode.value === 'series'
      && continuitySeriesId.value != null;
    const seriesId = continuitySeriesId.value ?? activeStorySeries.value?.id ?? null;
    const scope: ContinuityScope = (hasSeriesReports || continuityInSeriesMode) && seriesId != null
      ? { mode: 'series', seriesId }
      : { mode: 'manuscript' };
    analysisCtx.runCraftAnalysis(folder, toRun, scope, seriesId ?? undefined);
  } else {
    analysisCtx.runAnalyze(folder, forceResummarize.value, 'kdp', toRun, {
      publishEbook: publishEbook.value,
      publishPrint: publishPrint.value,
    });
  }
}

function onMarketIntel(): void {
  const folder = storiesCtx.activeFolder.value;
  hasRun.value = true;
  analysisCtx.runMarketIntel(folder);
}

function onStop(): void {
  analysisCtx.cancelOperation();
}

async function onRefreshSummaries(): Promise<void> {
  const folder = storiesCtx.activeFolder.value;
  if (!folder) return;
  hasRun.value = true;
  analysisCtx.summaryRunActive.value = true;
  await analysisCtx.runSummaries(folder);
}

function summaryFileStatus(file: string): string {
  return analysisCtx.summaryFileProgress.value[file] || '';
}

function summaryFileMarker(file: string): string {
  const status = summaryFileStatus(file);
  if (status === 'done' || status === 'skipped') return '✓';
  if (status === 'active') return '…';
  if (status === 'pending') return '○';
  return '';
}
</script>

<template>
  <div class="panel analyzer-panel">
    <h2 class="panel-title">Analyzer</h2>
    <p class="panel-desc">
      <template v-if="storiesCtx.activeStory.value">
        Story: {{ storiesCtx.activeStory.value.name }}
      </template>
      <template v-else>
        Select or create a story to enable reports.
      </template>
    </p>

    <AnalyzerPlatformTabs />

    <div v-if="platformCtx.isKdp.value" class="publish-formats-row">
      <span>Publishing formats:</span>
      <label><input v-model="publishEbook" type="checkbox" /> Ebook</label>
      <label><input v-model="publishPrint" type="checkbox" /> Print</label>
    </div>

    <div v-if="setupIssues.length > 0" class="setup-alert setup-alert--warning">
      <div class="setup-alert-title">Setup required before running reports</div>
      <ul class="setup-alert-list">
        <li v-for="issue in setupIssues" :key="issue.id">{{ issue.message }}</li>
      </ul>
      <button type="button" class="btn btn-secondary btn-small" @click="openSettings">Open Settings</button>
    </div>

    <div
      v-if="platformCtx.isKdp.value && marketIntelSetupIssues.length > 0"
      class="setup-alert setup-alert--info"
    >
      <div class="setup-alert-title">Market Intel also needs</div>
      <ul class="setup-alert-list">
        <li v-for="issue in marketIntelSetupIssues" :key="`mi-${issue.id}`">{{ issue.message }}</li>
      </ul>
    </div>

    <!-- Actions (top) -->
    <div class="analyzer-actions">
      <button
        class="btn"
        :disabled="getReportsDisabled"
        @click="onGetReports"
      >Get Reports</button>

      <span v-if="reportsToRun.length > 0 && formatTotalCost()" class="cost-total">
        {{ formatTotalCost() }}
      </span>

      <button
        v-if="platformCtx.isKdp.value"
        class="btn btn-secondary"
        title="Run market intelligence via Canopy API"
        :disabled="analysisCtx.isWorking.value || !canSelectReports || !analysisCtx.analysisState.value?.has_search_terms || marketIntelSetupIssues.length > 0"
        @click="onMarketIntel"
      >Market Intel</button>

      <button
        v-if="analysisCtx.isWorking.value"
        class="btn btn-stop"
        @click="onStop"
      >Stop</button>

      <label
        v-if="platformCtx.platform.value !== 'craft' && platformCtx.platform.value !== 'publish'"
        class="force-resummarize-label"
      >
        <input v-model="forceResummarize" type="checkbox" :disabled="reportsLocked" />
        Force re-scan
      </label>
    </div>

    <div class="summary-status-row">
      <span
        class="summary-status-text"
        :class="{ 'summary-status-warning': summaryStatus.needsRefresh }"
      >{{ summaryStatus.text }}</span>
      <button
        type="button"
        class="btn btn-secondary btn-small"
        :disabled="analysisCtx.isWorking.value || !storiesCtx.activeFolder.value || setupIssues.length > 0"
        @click="onRefreshSummaries"
      >Refresh Summaries</button>
    </div>

    <div v-if="summaryStatus.needsRefresh || Object.keys(analysisCtx.summaryFileProgress.value).length" class="summary-issues">
      <div v-if="summaryIssueFiles.missing.length > 0 || Object.keys(analysisCtx.summaryFileProgress.value).length">
        <span v-if="summaryIssueFiles.missing.length > 0" class="summary-issues-label">Missing summaries:</span>
        <ul v-if="summaryIssueFiles.missing.length > 0" class="summary-file-list">
          <li
            v-for="f in summaryIssueFiles.missing"
            :key="`missing-${f}`"
            class="summary-file-item"
            :class="{
              'summary-file-done': summaryFileStatus(f) === 'done' || summaryFileStatus(f) === 'skipped',
              'summary-file-active': summaryFileStatus(f) === 'active',
            }"
          >
            <span aria-hidden="true">{{ summaryFileMarker(f) }}</span>
            {{ f }}
          </li>
        </ul>
      </div>
      <div v-if="summaryIssueFiles.stale.length > 0" class="summary-stale-block">
        <span class="summary-issues-label">Changed since last scan:</span>
        <ul class="summary-file-list">
          <li
            v-for="f in summaryIssueFiles.stale"
            :key="`stale-${f}`"
            class="summary-file-item"
            :class="{
              'summary-file-done': summaryFileStatus(f) === 'done' || summaryFileStatus(f) === 'skipped',
              'summary-file-active': summaryFileStatus(f) === 'active',
            }"
          >
            <span aria-hidden="true">{{ summaryFileMarker(f) }}</span>
            {{ f }}
          </li>
        </ul>
      </div>
    </div>

    <div
      v-if="(platformCtx.platform.value === 'craft' || platformCtx.platform.value === 'publish') && (selected.includes('continuity_check') || selected.some(id => seriesReportIds.includes(id)))"
      class="continuity-scope-row"
    >
      <span class="continuity-scope-label">Continuity Check scope:</span>
      <label class="scope-radio">
        <input v-model="continuityScopeMode" type="radio" value="manuscript" />
        This manuscript
      </label>
      <label class="scope-radio">
        <input v-model="continuityScopeMode" type="radio" value="series" :disabled="seriesCtx.series.value.length === 0" />
        Series
      </label>
      <select
        v-if="continuityScopeMode === 'series'"
        v-model="continuitySeriesId"
        class="continuity-series-select"
      >
        <option :value="null" disabled>Choose a series…</option>
        <option v-for="s in seriesCtx.series.value" :key="s.id" :value="s.id">{{ s.name }} ({{ s.books.length }} books)</option>
      </select>
      <span v-if="seriesCtx.series.value.length === 0" class="continuity-scope-hint">No series yet — create one in the Series panel.</span>
    </div>

    <!-- Report cards -->
    <div v-if="!reportTypesLoaded && !loadError" class="report-cards-empty">
      Loading report catalog…
    </div>
    <div v-else-if="loadError" class="report-cards-empty report-cards-error">
      Could not load report catalog: {{ loadError }}.
    </div>
    <div v-else-if="visibleReports.length === 0" class="report-cards-empty">
      No report types for the {{ platformCtx.platform.value }} tab. If this persists, check Admin → SQL:
      <code>SELECT COUNT(*) FROM lore.report_types;</code>
    </div>
    <div v-else class="report-cards">
      <template v-for="section in reportSections" :key="section.id">
        <div v-if="section.showHeader" class="report-section-header">
          <label class="craft-group-select">
            <input
              type="checkbox"
              :checked="groupSelectionState(section.reports) === 'all'"
              :disabled="reportsLocked || section.disabled"
              @change="toggleGroupSelection(section.reports, reportsLocked || section.disabled)"
            />
          </label>
          <div class="craft-group-titles">
            <div class="report-section-title">{{ section.label }}</div>
            <div class="report-section-subtitle">{{ section.subtitle }}</div>
          </div>
        </div>
        <div
          v-for="report in section.reports"
          :key="report.id"
          class="report-card"
          :class="{
            disabled: reportsLocked || section.disabled || tierForReport(report) === 'locked',
          }"
        >
          <div class="report-card-check">
            <input
              type="checkbox"
              :checked="selected.includes(report.id)"
              :disabled="reportsLocked || section.disabled || tierForReport(report) === 'locked'"
              @change="toggleReport(report.id, reportsLocked || section.disabled || tierForReport(report) === 'locked')"
            />
          </div>
          <div class="report-card-content">
            <div class="report-card-label-row">
              <div class="report-card-label">{{ report.label }}</div>
              <span
                class="tier-badge"
                :class="`tier-badge--${tierForReport(report)}`"
              >{{ reportAccessLabel(tierForReport(report)) }}</span>
            </div>
            <div class="report-card-desc">{{ report.description }}</div>
            <div class="report-card-meta">
              <span
                v-if="report.freshness === 'fresh'"
                class="report-card-status report-card-status--fresh"
              >has run</span>
              <span
                v-else-if="report.freshness === 'stale'"
                class="report-card-status report-card-status--stale"
              >stale — re-run to refresh</span>
              <span class="report-card-cost">{{ reportRunCost(report.id, isAiReport(report.id)) }}</span>
            </div>
            <div
              v-if="selected.includes(report.id) && prerequisitesForReport(report.id).length > 0"
              class="report-deps"
            >
              <div class="report-deps-heading">Also runs</div>
              <div
                v-for="dep in prerequisitesForReport(report.id)"
                :key="`${report.id}-${dep.id}`"
                class="report-dep-row"
              >
                <input
                  type="checkbox"
                  :checked="isDepInRunQueue(dep.id)"
                  @change="onDepCheckboxChange(dep.id, $event)"
                />
                <div class="report-dep-body">
                  <div class="report-dep-title-row">
                    <span class="report-dep-label">{{ dep.label }}</span>
                    <span class="report-dep-cost">
                      {{ isDepInRunQueue(dep.id) ? reportRunCost(dep.id, isAiReport(dep.id)) : '—' }}
                    </span>
                  </div>
                  <span
                    class="report-dep-status"
                    :class="{
                      'report-card-status--fresh': dep.freshness === 'fresh',
                      'report-card-status--stale': dep.freshness === 'stale',
                    }"
                  >{{ depStatusLabel(dep.freshness) }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- Activity indicator -->
    <div
      v-if="(hasRun || analysisCtx.summaryRunActive.value) && analysisCtx.isWorking.value"
      class="activity-indicator"
    >
      <div class="spinner"></div>
      <span class="activity-text">Working...</span>
    </div>

    <!-- Log output (only shown after first run) -->
    <LogStream v-if="hasRun || analysisCtx.summaryRunActive.value" />
  </div>
</template>

<style scoped>
.analyzer-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--content-pad, 20px);
  overflow-y: auto;
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin-bottom: 10px;
}

.panel-desc {
  color: var(--text-muted);
  margin-bottom: 14px;
  font-size: 13px;
  line-height: 1.5;
}

.report-section-header {
  grid-column: 1 / -1;
  display: flex;
  align-items: flex-start;
  gap: 10px;
  margin-top: 8px;
  margin-bottom: 4px;
}

.craft-group-select {
  display: flex;
  align-items: center;
  padding-top: 2px;
  cursor: pointer;
}

.craft-group-select input[type="checkbox"] {
  accent-color: var(--accent);
  width: 15px;
  height: 15px;
  cursor: pointer;
}

.craft-group-titles {
  flex: 1;
  min-width: 0;
}

.report-section-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text);
}

.report-section-subtitle {
  font-size: 12px;
  color: var(--text-muted);
}

.publish-formats-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--text-muted);
}

.publish-formats-row label {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--text);
  cursor: pointer;
}

.publish-formats-row input[type="checkbox"] {
  accent-color: var(--accent);
}

.setup-alert {
  margin-bottom: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  font-size: 13px;
}

.setup-alert--warning {
  border-color: color-mix(in srgb, var(--warning, #d4a017) 45%, var(--border));
  background: color-mix(in srgb, var(--warning, #d4a017) 8%, var(--surface));
}

.setup-alert--info {
  border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
  background: color-mix(in srgb, var(--accent) 6%, var(--surface));
}

.setup-alert-title {
  font-weight: 600;
  color: var(--text);
  margin-bottom: 6px;
}

.setup-alert-list {
  margin: 0 0 10px;
  padding-left: 1.2em;
  color: var(--text-muted);
}

.report-deps {
  margin-top: 10px;
  padding-top: 10px;
  padding-left: 12px;
  border-top: 1px solid var(--border);
  border-left: 2px solid color-mix(in srgb, var(--accent) 35%, var(--border));
}

.report-deps-heading {
  font-size: 11px;
  margin-bottom: 6px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-muted);
}

.report-dep-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 4px 0;
}

.report-dep-row + .report-dep-row {
  border-top: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
}

.report-dep-row input[type="checkbox"] {
  accent-color: var(--accent);
  width: 14px;
  height: 14px;
  margin-top: 2px;
  cursor: pointer;
}

.report-dep-body {
  flex: 1;
  min-width: 0;
}

.report-dep-title-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.report-dep-label {
  font-size: 12px;
  color: var(--text);
}

.report-dep-cost {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}

.report-dep-status {
  display: block;
  font-size: 11px;
  margin-top: 2px;
  color: var(--text-muted);
}

/* ── Platform tabs (legacy) ──────────────────────────────────────────────── */

.platform-tabs {
  display: flex;
  gap: 0;
  margin-bottom: 14px;
  border-bottom: 2px solid var(--border);
}

.platform-tab {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 600;
  padding: 8px 16px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -2px;
  transition: color 0.15s, border-color 0.15s;
}

.platform-tab:hover {
  color: var(--text);
}

.platform-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

/* ── Report cards ──────────────────────────────────────────────────────────── */

.report-cards {
  flex: 1;
  overflow-y: auto;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 8px;
  margin-bottom: 14px;
  padding-right: 4px;
  align-content: start;
  width: 100%;
  min-width: 0;
}

.report-cards-empty {
  flex: 1;
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1.5;
  padding: 12px 0;
  margin-bottom: 14px;
}

.report-cards-error {
  color: var(--danger);
}

.report-card {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  transition: border-color 0.15s, opacity 0.15s;
}

.report-card:hover {
  border-color: var(--accent);
}

.report-card.dimmed {
  opacity: 0.55;
}

.report-card.dimmed:hover {
  border-color: var(--border);
}

.report-card.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.report-card.disabled:hover {
  border-color: var(--border);
}

.report-card-check {
  display: flex;
  align-items: center;
  padding-top: 2px;
  cursor: pointer;
}

.report-card-check input[type="checkbox"] {
  accent-color: var(--accent);
  width: 15px;
  height: 15px;
  cursor: pointer;
}

.report-card.dimmed .report-card-check input[type="checkbox"] {
  cursor: not-allowed;
}

.report-card-content {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.report-card-label-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.report-card-label-row :deep(.tier-badge) {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  line-height: 1.3;
}

.report-card-label-row :deep(.tier-badge--free) {
  color: var(--color-tier-free);
  background: color-mix(in srgb, var(--color-tier-free) 14%, transparent);
}

.report-card-label-row :deep(.tier-badge--subscriber) {
  color: var(--color-tier-subscriber);
  background: color-mix(in srgb, var(--color-tier-subscriber) 14%, transparent);
}

.report-card-label-row :deep(.tier-badge--locked) {
  color: var(--color-text-tertiary);
  background: color-mix(in srgb, var(--color-tier-locked) 35%, transparent);
}

.report-card-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.report-card-desc {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.4;
}

.report-card-status {
  font-size: 11px;
  font-weight: 500;
}

.report-card-status--fresh {
  color: var(--success, #3d9970);
}

.report-card-status--stale {
  color: var(--warning, #d4a017);
}

.report-card-meta {
  display: flex;
  gap: 10px;
  align-items: center;
  margin-top: 2px;
}

.report-card-cost {
  font-size: 11px;
  color: var(--text-muted);
  font-weight: 500;
}

.cost-total {
  font-size: 12px;
  color: var(--text-muted);
  font-weight: 600;
  padding: 4px 10px;
  background: var(--surface2);
  border-radius: var(--radius);
}

.summary-status-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}

.summary-status-text {
  font-size: 13px;
  color: var(--text-muted);
}

.summary-status-warning {
  color: var(--warning, #d4a017);
}

.btn-small {
  padding: 4px 10px;
  font-size: 12px;
}

.summary-issues {
  margin-bottom: 12px;
  font-size: 12px;
  color: var(--text-muted);
}

.summary-issues-label {
  display: block;
  margin-bottom: 4px;
}

.summary-stale-block {
  margin-top: 8px;
}

.summary-file-list {
  margin: 0;
  padding-left: 0;
  list-style: none;
}

.summary-file-item {
  display: flex;
  gap: 6px;
  padding: 2px 0;
}

.summary-file-done {
  color: #1a7f37;
}

.summary-file-active {
  color: var(--accent);
  font-weight: 600;
}

/* ── Actions ───────────────────────────────────────────────────────────────── */

.analyzer-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}

.btn {
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--color-on-accent);
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
  padding: 9px 18px;
  transition: background 0.15s;
}

.btn:hover {
  background: var(--accent-dim);
}

.btn:disabled {
  background: var(--surface2);
  color: var(--text-muted);
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.btn-secondary:hover:not(:disabled) {
  color: var(--text);
  border-color: var(--accent);
}

.btn-stop {
  background: var(--danger);
  color: var(--color-on-accent);
  font-size: 12px;
  padding: 9px 12px;
  border-radius: var(--radius);
  border: none;
  cursor: pointer;
  white-space: nowrap;
}

.btn-stop:hover {
  background: var(--color-accent-hover);
}

.force-resummarize-label {
  font-size: 12px;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
  cursor: pointer;
  margin-left: auto;
}

.force-resummarize-label input[type="checkbox"] {
  accent-color: var(--accent);
}

/* ── Continuity scope row ────────────────────────────────────────────────── */

.continuity-scope-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  margin-bottom: 10px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-size: 12px;
}

.continuity-scope-label {
  color: var(--text-muted);
  font-weight: 600;
  white-space: nowrap;
}

.scope-radio {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
}

.scope-radio input[type="radio"] {
  accent-color: var(--accent);
}

.continuity-series-select {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  padding: 5px 8px;
  font-size: 12px;
}

.continuity-scope-hint {
  color: var(--text-muted);
  font-size: 11px;
}

/* ── Activity indicator ────────────────────────────────────────────────────── */

.activity-indicator {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  padding: 6px 12px;
  background: var(--color-accent-subtle);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-size: 12px;
  color: var(--accent);
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid color-mix(in srgb, var(--color-accent) 30%, transparent);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.activity-text {
  font-weight: 500;
}
</style>
