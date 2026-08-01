<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue';
import { useAiSpend } from '../composables/useAiSpend';
import { useAnalysis } from '../composables/useAnalysis';
import { useServiceHealth } from '../composables/useServiceHealth';

const { totals, refreshAiSpend } = useAiSpend();
const { isWorking } = useAnalysis();
const { serviceHealth, checkHealth } = useServiceHealth();

const services = computed(() => [
  {
    key: 'ai',
    label: 'AI',
    ...serviceHealth.value.ai,
  },
  {
    key: 'canopy',
    label: 'Canopy',
    ...serviceHealth.value.canopy,
  },
  {
    key: 'dataforseo',
    label: 'DFS',
    ...serviceHealth.value.dataforseo,
  },
]);

function dotClass(svc: { configured: boolean; connected: boolean; checking: boolean }): string {
  if (svc.checking) return 'dot--gray pulse';
  if (!svc.configured) return 'dot--gray';
  if (svc.connected) return 'dot--green';
  return 'dot--red';
}

function tooltip(svc: { configured: boolean; connected: boolean; error: string | null }): string {
  if (!svc.configured) return 'Not configured';
  if (svc.connected) return 'Connected';
  return svc.error || 'Disconnected';
}

function formatUsd(amount: number): string {
  if (amount === 0) return '$0.00';
  if (amount < 0.01) return '<$0.01';
  return `$${amount.toFixed(2)}`;
}

let refreshTimer: ReturnType<typeof setInterval> | undefined;

function startPolling(): void {
  if (refreshTimer) return;
  refreshTimer = setInterval(() => void refreshAiSpend(), 3000);
}

function stopPolling(): void {
  if (!refreshTimer) return;
  clearInterval(refreshTimer);
  refreshTimer = undefined;
}

onMounted(() => {
  void refreshAiSpend();
});

watch(isWorking, (working) => {
  if (working) {
    void refreshAiSpend();
    startPolling();
  } else {
    stopPolling();
    void refreshAiSpend();
  }
}, { immediate: true });

onUnmounted(() => {
  stopPolling();
});
</script>

<template>
  <footer id="status-footer">
    <span class="status-label">AI spend</span>
    <span class="status-value">{{ formatUsd(totals.month_usd) }}</span>
    <span class="status-sep">this month</span>
    <span class="status-divider" aria-hidden="true" />
    <span class="status-value">{{ formatUsd(totals.ytd_usd) }}</span>
    <span class="status-sep">YTD</span>
    <span class="status-divider" aria-hidden="true" />
    <span
      v-for="svc in services"
      :key="svc.key"
      class="health-dot-group"
      :title="tooltip(svc)"
      role="status"
      :aria-label="`${svc.label}: ${tooltip(svc)}`"
      @click="checkHealth"
    >
      <span class="health-dot" :class="dotClass(svc)" />
      <span class="health-label">{{ svc.label }}</span>
    </span>
  </footer>
</template>

<style scoped>
#status-footer {
  grid-area: footer;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  height: 24px;
  border-top: 1px solid var(--border);
  background: var(--surface);
  font-size: 11px;
  user-select: none;
  color: var(--text-muted);
}

.status-label {
  margin-right: 2px;
}

.status-value {
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  color: var(--text);
}

.status-sep {
  margin-right: 4px;
}

.status-divider {
  width: 1px;
  height: 12px;
  background: var(--border);
  margin: 0 6px;
}

.health-dot-group {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  cursor: pointer;
  padding: 0 4px;
}

.health-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot--green { background: var(--success); }
.dot--red { background: var(--danger); }
.dot--gray { background: var(--text-muted); opacity: 0.5; }

.pulse {
  animation: pulse-fade 1.2s ease-in-out infinite;
}

@keyframes pulse-fade {
  0%, 100% { opacity: 0.3; }
  50% { opacity: 1; }
}

.health-label {
  font-size: 10px;
}
</style>
