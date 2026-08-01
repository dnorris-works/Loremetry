<script setup lang="ts">
import { inject, computed } from 'vue';
import { storiesKey, campaignsKey } from '../../injectionKeys';
import type { AdCampaign } from '../../types';

const storiesCtx = inject(storiesKey)!;
const campaignsCtx = inject(campaignsKey)!;

const emit = defineEmits<{
  (e: 'open-campaign', id: number): void;
  (e: 'new-campaign'): void;
  (e: 'platform-accounts'): void;
}>();

const activeStory = computed(() => storiesCtx.activeStory.value);
const campaigns = computed(() => campaignsCtx.campaigns.value);

function statusClass(status: string): string {
  if (status === 'active') return 'tag-success';
  if (status === 'paused') return 'tag-warn';
  return 'tag-default';
}

function formatSpend(amount: number): string {
  return `$${amount.toFixed(2)}`;
}

function onOpen(c: AdCampaign): void {
  emit('open-campaign', c.id);
}
</script>

<template>
  <div class="marketing-panel">
    <div class="panel-header-row">
      <h2 class="panel-title">Ad Campaigns</h2>
      <div class="header-actions">
        <button type="button" class="btn btn-sm" @click="emit('platform-accounts')">Platform accounts</button>
        <button type="button" class="btn btn-sm" :disabled="!activeStory" @click="emit('new-campaign')">
          New campaign
        </button>
      </div>
    </div>

    <div class="panel-body-scroll">
      <p v-if="!activeStory" class="panel-desc">Select a story in the sidebar to manage ad campaigns.</p>

      <template v-else>
        <p class="panel-desc">
          Campaigns for <strong>{{ activeStory.name }}</strong>. Track creatives, metrics, spend, and audience tests.
        </p>

        <p v-if="campaigns.length === 0" class="panel-desc">No campaigns yet. Click New campaign to get started.</p>

        <div v-else class="campaign-list">
          <button
            v-for="c in campaigns"
            :key="c.id"
            type="button"
            class="campaign-card"
            @click="onOpen(c)"
          >
            <div class="campaign-card-top">
              <span class="campaign-name">{{ c.name }}</span>
              <span class="tag" :class="statusClass(c.status)">{{ c.status }}</span>
            </div>
            <div class="campaign-meta">
              <span v-if="c.platform">{{ c.platform }}</span>
              <span v-if="c.objective"> · {{ c.objective }}</span>
            </div>
            <div class="campaign-meta">
              <span v-if="c.total_spend">Spent: {{ formatSpend(c.total_spend) }}</span>
              <span v-if="c.start_date">
                {{ c.start_date }}<template v-if="c.end_date"> – {{ c.end_date }}</template>
              </span>
            </div>
          </button>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.marketing-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.panel-header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 20px 24px 12px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin: 0;
}

.header-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.panel-body-scroll {
  flex: 1;
  overflow: auto;
  padding: 16px 24px 24px;
  max-width: 640px;
}

.campaign-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.campaign-card {
  text-align: left;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px;
  cursor: pointer;
  color: inherit;
}

.campaign-card:hover {
  border-color: var(--accent);
}

.campaign-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.campaign-name {
  font-weight: 600;
  font-size: 14px;
}

.campaign-meta {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 4px;
}

.tag {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--border);
  text-transform: capitalize;
}

.tag-success {
  border-color: var(--success);
  color: var(--success);
}

.tag-warn {
  border-color: var(--warning);
  color: var(--warning);
}

.tag-default {
  color: var(--text-muted);
}
</style>
