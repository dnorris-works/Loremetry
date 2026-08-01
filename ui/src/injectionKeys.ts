// Typed injection keys — compile-time safety for provide/inject.

import type { InjectionKey, Ref, ComputedRef } from 'vue';
import type {
  Story,
  StoriesResult,
  AnalysisState,
  ReportEnvelope,
  SidebarReportGroup,
  SidebarReport,
  Series,
  LogLine,
  Finding,
  ModelInfo,
  SummaryFileStatus,
  AdCampaign,
  AdCampaignDetail,
  AdCreative,
  AdLandingPage,
  AdPerformanceSnapshot,
  AdSpendEntry,
  AdAudienceNote,
  AdPlatformAccount,
} from './types';
import type { PlatformId } from './composables/usePlatform';
import type { ContinuityScope } from './composables/useAnalysis';
import type { ModelAssignments, FolderStructure, ThemeMode } from './composables/useSettings';

// ── Stories ───────────────────────────────────────────────────────────────────

export interface StoriesContext {
  stories: Ref<Story[]>;
  activeStoryId: Ref<string | null>;
  activeStory: ComputedRef<Story | null>;
  activeFolder: ComputedRef<string>;
  loadStories: () => Promise<void>;
  setActiveStory: (id: string | null) => void;
  addStory: (name: string) => Promise<StoriesResult>;
  initStory: (name: string) => Promise<StoriesResult>;
  updateStory: (id: string, name: string, biblePath?: string) => Promise<StoriesResult>;
  deleteStory: (id: string) => Promise<StoriesResult>;
}

export const storiesKey: InjectionKey<StoriesContext> = Symbol('stories');

// ── Analysis ──────────────────────────────────────────────────────────────────

export interface AnalysisContext {
  analysisState: Ref<AnalysisState | null>;
  isWorking: Ref<boolean>;
  logLines: Ref<LogLine[]>;
  summaryFileProgress: Ref<Record<string, SummaryFileStatus>>;
  summaryRunActive: Ref<boolean>;
  refreshState: (folder: string) => Promise<void>;
  runAnalyze: (
    folder: string,
    forceResummarize: boolean,
    platform: string,
    selected?: string[],
    formats?: { publishEbook: boolean; publishPrint: boolean },
  ) => Promise<void>;
  runCraftAnalysis: (
    folder: string,
    selected: string[],
    continuityScope: ContinuityScope,
    seriesIdForAudits?: number,
  ) => Promise<void>;
  runMarketIntel: (folder: string) => Promise<void>;
  runSummaries: (folder: string) => Promise<void>;
  cancelOperation: () => Promise<void>;
  clearLog: () => void;
  appendLog: (msg: string) => void;
}

export const analysisKey: InjectionKey<AnalysisContext> = Symbol('analysis');

// ── Platform ──────────────────────────────────────────────────────────────────

export interface PlatformContext {
  platform: Ref<PlatformId>;
  isKdp: ComputedRef<boolean>;
  setPlatform: (p: PlatformId) => void;
}

export const platformKey: InjectionKey<PlatformContext> = Symbol('platform');

// ── Settings ──────────────────────────────────────────────────────────────────

export interface SettingsContext {
  theme: Ref<ThemeMode>;
  setTheme: (mode: ThemeMode) => void;
  provider: Ref<string>;
  model: ComputedRef<string>;
  proseModel: ComputedRef<string>;
  modelAssignments: Ref<ModelAssignments>;
  modelFor: (fn: keyof ModelAssignments) => string;
  models: Ref<ModelInfo[]>;
  folderStructure: Ref<FolderStructure>;
  fetchModels: () => Promise<{ success: boolean; error: string }>;
  saveSettings: () => Promise<void>;
  testCanopy: () => Promise<{ success: boolean; error: string }>;
  testDataforseo: () => Promise<{ success: boolean; error: string }>;
}

export const settingsKey: InjectionKey<SettingsContext> = Symbol('settings');

// ── Reports ───────────────────────────────────────────────────────────────────

export interface ReportsContext {
  sidebarGroups: Ref<SidebarReportGroup[]>;
  savedReports: Ref<SidebarReport[]>;
  currentReport: Ref<ReportEnvelope | null>;
  loadSidebarReports: (folder: string, platform: string) => Promise<void>;
  loadSavedReports: (folder: string) => Promise<void>;
  openReport: (id: number) => Promise<ReportEnvelope>;
  deleteReport: (id: number) => Promise<void>;
  closeReport: () => void;
}

export const reportsKey: InjectionKey<ReportsContext> = Symbol('reports');

// ── Series ────────────────────────────────────────────────────────────────────

export interface SeriesContext {
  series: Ref<Series[]>;
  loadSeries: () => Promise<void>;
}

export const seriesKey: InjectionKey<SeriesContext> = Symbol('series');

// ── Marketing / campaigns ─────────────────────────────────────────────────────

export interface CampaignsContext {
  campaigns: Ref<AdCampaign[]>;
  platformAccounts: Ref<AdPlatformAccount[]>;
  landingPages: Ref<AdLandingPage[]>;
  campaignDetail: Ref<AdCampaignDetail | null>;
  loadCampaigns: (storyFolder: string) => Promise<void>;
  createCampaign: (storyFolder: string, name: string, platform: string, objective: string) => Promise<{ success: boolean; id: number; error: string }>;
  updateCampaign: (request: {
    id: number;
    name: string;
    platform: string;
    platform_account_id: number | null;
    objective: string;
    status: string;
    budget: number | null;
    budget_period: string;
    start_date: string;
    end_date: string;
    target_audience: string;
    landing_page_id: number | null;
    notes: string;
  }) => Promise<{ success: boolean; error: string }>;
  deleteCampaign: (id: number) => Promise<{ success: boolean; error: string }>;
  loadCampaignDetail: (id: number) => Promise<AdCampaignDetail | null>;
  createCreative: (request: Omit<AdCreative, 'id' | 'created_at' | 'updated_at'>) => Promise<{ success: boolean; id: number; error: string }>;
  updateCreative: (request: Omit<AdCreative, 'created_at' | 'updated_at'>) => Promise<{ success: boolean; error: string }>;
  deleteCreative: (id: number) => Promise<{ success: boolean; error: string }>;
  addPerformanceSnapshot: (request: Omit<AdPerformanceSnapshot, 'id' | 'created_at'>) => Promise<{ success: boolean; id: number; error: string }>;
  deletePerformanceSnapshot: (id: number) => Promise<{ success: boolean; error: string }>;
  addSpendEntry: (request: Omit<AdSpendEntry, 'id' | 'created_at'>) => Promise<{ success: boolean; id: number; error: string }>;
  deleteSpendEntry: (id: number) => Promise<{ success: boolean; error: string }>;
  loadLandingPages: (storyFolder: string) => Promise<void>;
  createLandingPage: (request: { story_folder: string; name: string; url: string; conversion_rate: number | null; notes: string }) => Promise<{ success: boolean; id: number; error: string }>;
  updateLandingPage: (request: { id: number; name: string; url: string; conversion_rate: number | null; notes: string }, storyFolder: string) => Promise<{ success: boolean; error: string }>;
  deleteLandingPage: (id: number, storyFolder: string) => Promise<{ success: boolean; error: string }>;
  addAudienceNote: (request: Omit<AdAudienceNote, 'id' | 'created_at'>) => Promise<{ success: boolean; id: number; error: string }>;
  updateAudienceNote: (request: Omit<AdAudienceNote, 'created_at'>) => Promise<{ success: boolean; error: string }>;
  deleteAudienceNote: (id: number) => Promise<{ success: boolean; error: string }>;
  loadPlatformAccounts: () => Promise<void>;
  createPlatformAccount: (request: { platform: string; account_id: string; pixel_id: string; tracking_notes: string; payment_notes: string }) => Promise<{ success: boolean; id: number; error: string }>;
  updatePlatformAccount: (request: { id: number; platform: string; account_id: string; pixel_id: string; tracking_notes: string; payment_notes: string }) => Promise<{ success: boolean; error: string }>;
  deletePlatformAccount: (id: number) => Promise<{ success: boolean; error: string }>;
}

export const campaignsKey: InjectionKey<CampaignsContext> = Symbol('campaigns');

// ── Panel navigation ──────────────────────────────────────────────────────────

export const showPanelKey: InjectionKey<(name: string) => void> = Symbol('showPanel');

// ── Manuscript editor ─────────────────────────────────────────────────────────

export const openManuscriptEditorKey: InjectionKey<(findings: Finding[], startIndex: number) => void> = Symbol('openManuscriptEditor');
