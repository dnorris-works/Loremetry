<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { useAiSpend } from '../composables/useAiSpend';
import { useAnalysis } from '../composables/useAnalysis';

const { totals, refreshAiSpend } = useAiSpend();
const { isWorking } = useAnalysis();

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
</style>
