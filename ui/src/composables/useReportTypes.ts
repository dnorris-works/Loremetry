import { ref } from 'vue';
import { invoke } from '../api';
import type { ReportTypeDef } from '../types';

const reportTypes = ref<ReportTypeDef[]>([]);
const loaded = ref(false);
const loadError = ref('');

async function loadReportTypes(options?: { force?: boolean }): Promise<void> {
  if (loaded.value && !options?.force) return;
  loadError.value = '';
  try {
    const rows = await invoke<ReportTypeDef[]>('list_report_types_cmd');
    reportTypes.value = Array.isArray(rows) ? rows : [];
    loaded.value = true;
  } catch (e) {
    loaded.value = false;
    loadError.value = e instanceof Error ? e.message : String(e);
    console.error('Failed to load report types:', e);
  }
}

/**
 * Get the dependants for a report — the reports that must also be selected/deselected.
 * This is just the depends_on array from the DB record.
 */
function getDependants(id: string): string[] {
  const def = reportTypes.value.find(r => r.id === id);
  return def ? def.depends_on : [];
}

export function useReportTypes() {
  return {
    reportTypes,
    loaded,
    loadError,
    loadReportTypes,
    getDependants,
  };
}
