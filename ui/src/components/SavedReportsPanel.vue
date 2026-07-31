<script setup lang="ts">
import { inject, computed, watch } from 'vue';
import AnalyzerPlatformTabs from './AnalyzerPlatformTabs.vue';
import { storiesKey, reportsKey, showPanelKey, analysisKey } from '../injectionKeys';

const storiesCtx = inject(storiesKey)!;
const reportsCtx = inject(reportsKey)!;
const showPanel = inject(showPanelKey)!;
const analysisCtx = inject(analysisKey)!;

async function onReportClick(id: number): Promise<void> {
  await reportsCtx.openReport(id);
  showPanel('reports');
}

async function onDeleteReport(id: number): Promise<void> {
  if (!confirm('Delete this saved report? This cannot be undone.')) return;
  try {
    await reportsCtx.deleteReport(id);
    const folder = storiesCtx.activeFolder.value;
    if (folder) {
      await analysisCtx.refreshState(folder);
      await reportsCtx.loadSavedReports(folder);
    }
  } catch (err) {
    console.error(err);
  }
}

const reportMenuOptions = computed(() => reportsCtx.savedReports.value);

watch(() => storiesCtx.activeFolder.value, (folder) => {
  void reportsCtx.loadSavedReports(folder ?? '');
}, { immediate: true });
</script>

<template>
  <div class="saved-reports-root">
    <p class="panel-desc">
      <template v-if="storiesCtx.activeStory.value">
        Story: {{ storiesCtx.activeStory.value.name }}
      </template>
      <template v-else>
        Select or create a story to begin.
      </template>
    </p>

    <AnalyzerPlatformTabs />

    <div class="saved-reports-scroll">
      <p v-if="!storiesCtx.activeFolder.value" class="muted">
        Select a story to see saved reports.
      </p>
      <p v-else-if="reportsCtx.savedReports.value.length === 0" class="muted">
        No saved reports yet. Run reports from KDP/Wide, Craft, or Publish.
      </p>
      <ul v-else class="saved-report-list">
        <li v-for="report in reportMenuOptions" :key="report.id" class="saved-report-item">
          <button type="button" class="saved-report-label" @click="onReportClick(report.id)">
            {{ report.label }}
          </button>
          <button type="button" class="saved-report-delete" @click="onDeleteReport(report.id)">×</button>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.saved-reports-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--content-pad, 20px);
  overflow: hidden;
}

.panel-desc {
  color: var(--text-muted);
  margin-bottom: 12px;
  font-size: 13px;
}

.saved-reports-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.muted {
  color: var(--text-muted);
  font-size: 13px;
}

.saved-report-list {
  list-style: none;
  padding: 0;
  margin: 0;
}

.saved-report-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}

.saved-report-label {
  flex: 1;
  background: none;
  border: none;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  font-size: 13px;
  padding: 0;
}

.saved-report-label:hover {
  color: var(--accent);
}

.saved-report-delete {
  background: none;
  border: none;
  color: var(--danger);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
}
</style>
