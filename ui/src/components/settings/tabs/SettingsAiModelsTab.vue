<script setup lang="ts">
import { inject, ref, computed, onMounted } from 'vue';
import { settingsKey } from '../../../injectionKeys';
import type { ModelInfo } from '../../../types';
import type { ModelAssignments } from '../../../composables/useSettings';
import { useReportTypes } from '../../../composables/useReportTypes';

const settingsCtx = inject(settingsKey)!;
const { reportTypes, loadReportTypes } = useReportTypes();

type ModelSort = 'price' | 'provider';
const modelSort = ref<ModelSort>('price');

onMounted(() => {
  void loadReportTypes();
  void settingsCtx.ensureModelsLoaded();
});

const sortedModels = computed(() => {
  return [...settingsCtx.models.value].sort((a, b) => {
    if (modelSort.value === 'provider') {
      const provA = a.owned_by.toLowerCase();
      const provB = b.owned_by.toLowerCase();
      if (provA !== provB) return provA.localeCompare(provB);
    }
    const priceA = a.input_price ?? Infinity;
    const priceB = b.input_price ?? Infinity;
    return priceA - priceB;
  });
});

type Tier = 'basic' | 'capable' | 'strong';
const TIER_RANK: Record<Tier, number> = { basic: 0, capable: 1, strong: 2 };

function modelTier(m: ModelInfo): Tier {
  const price = m.input_price ?? 0;
  if (price <= 0.001) return 'basic';
  if (price <= 0.01) return 'capable';
  return 'strong';
}

function minTierFor(fnKey: string): Tier {
  if (fnKey === 'prose') return 'strong';
  const tiers = reportTypes.value
    .filter(r => r.model_slot === fnKey)
    .map(r => (r.min_tier as Tier) || 'basic');
  if (tiers.length === 0) return 'basic';
  return tiers.reduce((best, t) => (TIER_RANK[t] > TIER_RANK[best] ? t : best), 'basic' as Tier);
}

function modelFitLabel(m: ModelInfo, fnKey: string): string {
  const tier = modelTier(m);
  const min = minTierFor(fnKey);
  if (TIER_RANK[tier] >= TIER_RANK[min]) return ' ✓';
  return ' ⚠';
}

function fnOptionLabel(m: ModelInfo, fnKey: string): string {
  return m.id + modelFitLabel(m, fnKey);
}

function modelLabel(m: ModelInfo): string {
  let label = m.id;
  if (m.owned_by) label += ` (${m.owned_by})`;
  if (m.input_price != null && m.output_price != null) {
    label += ` — $${m.input_price}/$${m.output_price} per 1K tokens`;
  }
  return label;
}

const ASSIGNMENTS: { key: keyof ModelAssignments; label: string; hint: string }[] = [
  { key: 'summaries', label: 'Chapter Summaries', hint: 'Per-chapter genre signal extraction.' },
  { key: 'genre', label: 'Genre Analysis', hint: 'Classification and comps.' },
  { key: 'keywords', label: 'Keywords & Categories', hint: 'Short structured output.' },
  { key: 'continuity', label: 'Continuity Check', hint: 'Reasoning across chapters.' },
  { key: 'showDontTell', label: "Show Don't Tell", hint: 'Literary craft judgment.' },
  { key: 'aiIsms', label: 'AI-isms', hint: 'Synthetic prose detection.' },
  { key: 'prose', label: 'Prose Suggestions', hint: 'Creative rewriting — use a strong model.' },
];
</script>

<template>
  <div class="settings-form">
    <p class="panel-desc">
      The server auto-selects the cheapest model. You can override per function below.
    </p>

    <label class="field-label">Default model</label>
    <div class="model-row">
      <select v-model="settingsCtx.modelAssignments.value.default">
        <option v-if="sortedModels.length === 0" value="" disabled>Loading models…</option>
        <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ modelLabel(m) }}</option>
      </select>
    </div>

    <div v-if="sortedModels.length > 0" class="model-sort-row">
      <span class="model-sort-label">Sort:</span>
      <button type="button" class="model-sort-btn" :class="{ active: modelSort === 'price' }" @click="modelSort = 'price'">Price</button>
      <button type="button" class="model-sort-btn" :class="{ active: modelSort === 'provider' }" @click="modelSort = 'provider'">Provider</button>
    </div>

    <div v-if="sortedModels.length > 0" class="model-assignments">
      <div class="model-assign-header">Model per function</div>
      <div v-for="item in ASSIGNMENTS" :key="item.key" class="model-assign-row">
        <div class="model-assign-label">
          <strong>{{ item.label }}</strong>
          <span class="model-hint">{{ item.hint }}</span>
        </div>
        <select v-model="settingsCtx.modelAssignments.value[item.key]">
          <option value="">(Use default)</option>
          <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, item.key) }}</option>
        </select>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel-desc { font-size: 13px; color: var(--text-muted); line-height: 1.5; margin-bottom: 12px; }
.field-label { font-size: 12px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.06em; display: block; margin-bottom: 6px; }
.model-row { display: flex; gap: 8px; align-items: center; margin-bottom: 12px; }
.model-row select { flex: 1; background: var(--surface2); border: 1px solid var(--border); border-radius: var(--radius); color: var(--text); font-size: 13px; padding: 8px 10px; }
.model-hint { display: block; font-size: 11px; color: var(--text-muted); font-weight: 400; text-transform: none; letter-spacing: 0; margin-top: 2px; }
.model-sort-row { display: flex; align-items: center; gap: 6px; margin-bottom: 12px; }
.model-sort-label { font-size: 12px; color: var(--text-muted); }
.model-sort-btn { background: var(--surface2); border: 1px solid var(--border); border-radius: 4px; color: var(--text-muted); cursor: pointer; font-size: 11px; padding: 4px 8px; }
.model-sort-btn.active { border-color: var(--accent); color: var(--accent); }
.model-assignments { display: flex; flex-direction: column; gap: 10px; }
.model-assign-header { font-size: 13px; font-weight: 600; margin-bottom: 4px; }
.model-assign-row { display: grid; grid-template-columns: 1fr minmax(200px, 280px); gap: 12px; align-items: start; }
.model-assign-label { font-size: 13px; }
.model-assign-row select { background: var(--surface2); border: 1px solid var(--border); border-radius: var(--radius); color: var(--text); font-size: 13px; padding: 8px 10px; width: 100%; }
@media (max-width: 700px) {
  .model-assign-row { grid-template-columns: 1fr; }
}
</style>
