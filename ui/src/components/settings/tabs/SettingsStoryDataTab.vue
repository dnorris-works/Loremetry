<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue';
import { invoke } from '../../../api';
import { storiesKey, analysisKey, settingsKey } from '../../../injectionKeys';
import type { StoryArtifactStateResponse } from '../../../types';

const props = defineProps<{
  active?: boolean;
}>();

const storiesCtx = inject(storiesKey)!;
const analysisCtx = inject(analysisKey)!;
const settingsCtx = inject(settingsKey)!;

const state = ref<StoryArtifactStateResponse | null>(null);
const loading = ref(false);
const error = ref('');
const refreshing = ref(false);
const actionMsg = ref('');

const activeFolder = computed(() => storiesCtx.activeFolder.value);
const activeStoryName = computed(() => storiesCtx.activeStory.value?.name ?? '');

async function loadState(): Promise<void> {
  const folder = activeFolder.value;
  if (!folder) {
    state.value = null;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    state.value = await invoke<StoryArtifactStateResponse>('get_story_artifact_state', { folder });
  } catch (e) {
    error.value = String(e);
    state.value = null;
  } finally {
    loading.value = false;
  }
}

async function onRefreshSummaries(): Promise<void> {
  const folder = activeFolder.value;
  if (!folder) return;
  refreshing.value = true;
  actionMsg.value = '';
  try {
    const msg = await invoke<string>('refresh_chapter_summaries', {
      folder,
      provider: settingsCtx.provider.value,
      api_key: '',
      model: settingsCtx.modelFor('summaries') || settingsCtx.model.value,
    });
    actionMsg.value = msg || 'Chapter summaries refreshed.';
    await loadState();
    await analysisCtx.refreshState(folder);
  } catch (e) {
    actionMsg.value = 'Refresh failed: ' + String(e);
  } finally {
    refreshing.value = false;
  }
}

async function onClearSummaries(): Promise<void> {
  const folder = activeFolder.value;
  if (!folder) return;
  if (!confirm('Remove all stored chapter summaries for this story? Genre and keyword reports will need re-summarizing.')) {
    return;
  }
  actionMsg.value = '';
  try {
    await invoke<void>('clear_chapter_summaries', { folder });
    actionMsg.value = 'Chapter summaries cleared.';
    await loadState();
    await analysisCtx.refreshState(folder);
  } catch (e) {
    actionMsg.value = 'Clear failed: ' + String(e);
  }
}

watch([() => props.active, activeFolder], ([isActive]) => {
  if (isActive) void loadState();
}, { immediate: true });
</script>

<template>
  <div class="story-data-tab">
    <div class="story-data-header">
      <div>
        <strong>{{ activeStoryName || 'No story selected' }}</strong>
        <p v-if="state" class="meta">
          {{ state.chapter_count }} chapter summar{{ state.chapter_count === 1 ? 'y' : 'ies' }}
          <template v-if="state.manuscript_fingerprint">
            · fingerprint {{ state.manuscript_fingerprint.slice(0, 12) }}…
          </template>
        </p>
      </div>
      <div class="story-data-actions">
        <button type="button" class="btn btn-sm" :disabled="!activeFolder || refreshing" @click="onRefreshSummaries">
          {{ refreshing ? 'Refreshing…' : 'Refresh summaries' }}
        </button>
        <button type="button" class="btn btn-sm btn-danger" :disabled="!activeFolder" @click="onClearSummaries">
          Clear
        </button>
        <button type="button" class="btn btn-sm btn-secondary" :disabled="!activeFolder || loading" @click="loadState">
          Reload
        </button>
      </div>
    </div>

    <p v-if="error" class="error-msg">{{ error }}</p>
    <p v-if="actionMsg" class="status-msg">{{ actionMsg }}</p>

    <p class="panel-desc">
      Chapter summaries are AI-extracted genre signals. They power genre analysis, ranking, and keyword reports.
    </p>

    <div v-if="loading && !state" class="muted">Loading…</div>
    <p v-else-if="!activeFolder" class="muted">Select a story to view chapter summaries.</p>

    <table v-else-if="state && state.chapters.length > 0" class="data-table">
      <thead>
        <tr>
          <th>File</th>
          <th>Title</th>
          <th>Words</th>
          <th>Updated</th>
          <th>Preview</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="ch in state.chapters" :key="ch.file">
          <td>{{ ch.file }}</td>
          <td>{{ ch.title }}</td>
          <td>{{ ch.word_count }}</td>
          <td>{{ ch.updated_at }}</td>
          <td class="preview">{{ ch.summary_preview }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else-if="state" class="muted">No chapter summaries stored yet.</p>

    <div v-if="state && state.artifacts.length > 0" class="artifacts">
      <div class="artifacts-label">Artifact status</div>
      <div class="artifacts-list">
        <span v-for="[name, status] in state.artifacts" :key="name" class="artifact-chip">
          {{ name }}: <strong>{{ status }}</strong>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.story-data-header {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
  margin-bottom: 12px;
}
.meta { font-size: 12px; color: var(--text-muted); margin-top: 4px; }
.story-data-actions { display: flex; flex-wrap: wrap; gap: 8px; }
.panel-desc { font-size: 13px; color: var(--text-muted); line-height: 1.5; margin-bottom: 12px; }
.error-msg { color: var(--danger); font-size: 12px; margin-bottom: 8px; }
.status-msg { font-size: 12px; color: var(--text-muted); margin-bottom: 8px; }
.muted { font-size: 13px; color: var(--text-muted); }
.data-table { width: 100%; border-collapse: collapse; font-size: 12px; margin-bottom: 16px; }
.data-table th, .data-table td { border-bottom: 1px solid var(--border); padding: 6px 8px; text-align: left; vertical-align: top; }
.data-table th { color: var(--text-muted); font-weight: 600; }
.preview { max-width: 320px; color: var(--text-muted); }
.artifacts { margin-top: 12px; }
.artifacts-label { font-size: 12px; color: var(--text-muted); margin-bottom: 6px; }
.artifacts-list { display: flex; flex-wrap: wrap; gap: 8px 16px; }
.artifact-chip { font-size: 12px; color: var(--text-muted); }
.btn-secondary { background: var(--surface2); border: 1px solid var(--border); color: var(--text-muted); }
.btn-danger { background: var(--danger); color: var(--color-on-accent); border: none; }
</style>
