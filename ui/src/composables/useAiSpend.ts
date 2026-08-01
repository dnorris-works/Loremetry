import { ref } from 'vue';
import { invoke } from '../api';
import type { AiSpendTotals } from '../types';

const totals = ref<AiSpendTotals>({ month_usd: 0, ytd_usd: 0 });
const loaded = ref(false);

async function refreshAiSpend(): Promise<void> {
  try {
    totals.value = await invoke<AiSpendTotals>('get_ai_spend_totals');
    loaded.value = true;
  } catch (e) {
    console.error('get_ai_spend_totals:', e);
  }
}

export function useAiSpend() {
  return {
    totals,
    loaded,
    refreshAiSpend,
  };
}

export { refreshAiSpend };
