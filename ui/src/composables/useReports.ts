import { ref } from 'vue';
import { invoke } from '../api';
import type { ReportEnvelope, SidebarReportGroup, SidebarReport } from '../types';

const sidebarGroups = ref<SidebarReportGroup[]>([]);
const savedReports = ref<SidebarReport[]>([]);
const currentReport = ref<ReportEnvelope | null>(null);

async function loadSidebarReports(folder: string, platform: string): Promise<void> {
  if (!folder) {
    sidebarGroups.value = [];
    return;
  }
  try {
    sidebarGroups.value = await invoke<SidebarReportGroup[]>('get_sidebar_reports', { folder, platform });
  } catch (e) {
    console.error('loadSidebarReports:', e);
    sidebarGroups.value = [];
  }
}

async function loadSavedReports(folder: string): Promise<void> {
  if (!folder) {
    savedReports.value = [];
    return;
  }
  try {
    const groups = await invoke<SidebarReportGroup[]>('get_sidebar_reports', { folder, platform: 'saved' });
    savedReports.value = groups.flatMap(g =>
      g.versions.map(v => ({
        id: v.id,
        doc_type: g.doc_type,
        label: g.label,
        generated_at: v.generated_at,
      })),
    );
  } catch (e) {
    console.error('loadSavedReports:', e);
    savedReports.value = [];
  }
}

async function openReport(id: number): Promise<ReportEnvelope> {
  const envelope = await invoke<ReportEnvelope>('get_report_cmd', { id });
  currentReport.value = envelope;
  return envelope;
}

async function deleteReport(id: number): Promise<void> {
  await invoke<void>('delete_report_cmd', { id });
  savedReports.value = savedReports.value.filter(r => r.id !== id);
}

function closeReport(): void {
  currentReport.value = null;
}

export function useReports() {
  return {
    sidebarGroups,
    savedReports,
    currentReport,
    loadSidebarReports,
    loadSavedReports,
    openReport,
    deleteReport,
    closeReport,
  };
}
