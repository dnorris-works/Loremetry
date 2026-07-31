import { ref } from 'vue';
import { invoke } from '../api';

export interface CraftReportGroup {
  id: string;
  label: string;
  subtitle: string;
  reportIds: string[];
}

const craftReportGroups = ref<CraftReportGroup[]>([]);
const seriesReportIds = ref<string[]>([]);
const loaded = ref(false);

async function loadCraftReportGroups(): Promise<void> {
  if (loaded.value) return;
  try {
    const groups = await invoke<CraftReportGroup[]>('list_craft_report_groups_cmd');
    craftReportGroups.value = groups;
    seriesReportIds.value = groups.find(g => g.id === 'series')?.reportIds ?? [];
    loaded.value = true;
  } catch (e) {
    console.error('Failed to load craft report groups:', e);
  }
}

export function useCraftReportGroups() {
  return {
    craftReportGroups,
    seriesReportIds,
    loadCraftReportGroups,
  };
}
