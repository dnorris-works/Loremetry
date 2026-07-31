<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue';
import { invoke } from '../../../api';
import { storiesKey, reportsKey, showPanelKey } from '../../../injectionKeys';
import type { ArchivedReportRow } from '../../../types';

const props = defineProps<{
  active?: boolean;
}>();

const storiesCtx = inject(storiesKey)!;
const reportsCtx = inject(reportsKey)!;
const showPanel = inject(showPanelKey)!;

const rows = ref<ArchivedReportRow[]>([]);
const loading = ref(false);
const error = ref('');

const activeFolder = computed(() => storiesCtx.activeFolder.value);
const activeStoryName = computed(() => storiesCtx.activeStory.value?.name ?? '');

function formatTs(ts: string): string {
  if (!ts) return '—';
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleString(undefined, {
    month: 'short', day: 'numeric', year: 'numeric',
    hour: 'numeric', minute: '2-digit',
  });
}

async function loadArchived(): Promise<void> {
  const folder = activeFolder.value;
  if (!folder) {
    rows.value = [];
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    rows.value = await invoke<ArchivedReportRow[]>('get_archived_reports', { folder });
  } catch (e) {
    error.value = String(e);
    rows.value = [];
  } finally {
    loading.value = false;
  }
}

async function onRead(row: ArchivedReportRow): Promise<void> {
  try {
    await reportsCtx.openReport(row.id);
    showPanel('reports');
  } catch (e) {
    error.value = 'Could not open report: ' + String(e);
  }
}

async function onDelete(row: ArchivedReportRow): Promise<void> {
  if (!confirm(`Delete "${row.label}"? This cannot be undone.`)) return;
  try {
    await reportsCtx.deleteReport(row.id);
    await loadArchived();
  } catch (e) {
    error.value = 'Could not delete: ' + String(e);
  }
}

watch([() => props.active, activeFolder], ([isActive, folder]) => {
  if (isActive && folder) void loadArchived();
}, { immediate: true });
</script>

<template>
  <div class="archived-tab">
    <div class="archived-header">
      <strong v-if="activeStoryName">{{ activeStoryName }}</strong>
      <span v-else class="muted">No story selected</span>
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="!activeFolder || loading"
        @click="loadArchived"
      >
        Refresh
      </button>
    </div>

    <p v-if="error" class="error-msg">{{ error }}</p>

    <p v-if="!activeFolder" class="muted">Select a story to view archived reports.</p>
    <p v-else-if="loading && rows.length === 0" class="muted">Loading…</p>
    <p v-else-if="!loading && rows.length === 0" class="muted">No archived reports for this story.</p>

    <table v-else class="data-table">
      <thead>
        <tr>
          <th>Report</th>
          <th>Archived</th>
          <th>Reason</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.id">
          <td>{{ row.label }}</td>
          <td>{{ formatTs(row.archived_at) }}</td>
          <td>{{ row.archive_reason || '—' }}</td>
          <td class="actions">
            <button type="button" class="link-btn" @click="onRead(row)">Read</button>
            <button type="button" class="link-btn danger" @click="onDelete(row)">Delete</button>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.archived-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.muted { font-size: 13px; color: var(--text-muted); }
.error-msg { color: var(--danger); font-size: 12px; margin-bottom: 8px; }
.data-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.data-table th, .data-table td { border-bottom: 1px solid var(--border); padding: 8px; text-align: left; vertical-align: top; }
.data-table th { color: var(--text-muted); font-weight: 600; }
.actions { display: flex; gap: 8px; }
.link-btn { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 12px; padding: 0; }
.link-btn.danger { color: var(--danger); }
.btn-secondary { background: var(--surface2); border: 1px solid var(--border); color: var(--text-muted); }
</style>
