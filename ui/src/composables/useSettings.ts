import { ref, computed } from 'vue';
import { invoke, buildAuthHeaders, getOperatorBypassToken } from '../api';
import type { ModelInfo, ModelsResult } from '../types';

// ── AI function model assignments ─────────────────────────────────────────────
// Each AI function can have its own model. Empty means "use the default model."

export interface ModelAssignments {
  default:       string;  // Fallback for any function without a specific model
  summaries:     string;  // Chapter summaries (extraction)
  genre:         string;  // Genre analysis & ranking
  keywords:      string;  // Keywords, search terms, BISAC
  continuity:    string;  // Continuity checker (fact extraction + judgment)
  showDontTell:  string;  // Show Don't Tell analysis
  aiIsms:        string;  // AI-isms check
  prose:         string;  // Creative suggestions / rewrites
}

/** Default relative paths used when creating documents (client-side hints only). */
export interface FolderStructure {
  manuscript: string;
  bible: string;
  characters: string;
  locations: string;
  acts: string[];
  extra: string[];
}

const DEFAULT_FOLDER_STRUCTURE: FolderStructure = {
  manuscript: 'Manuscript',
  bible: 'Bible',
  characters: 'Characters',
  locations: 'Locations',
  acts: ['Act-1', 'Act-2', 'Act-3'],
  extra: ['Publishing/Cover', 'Research'],
};

export function manuscriptActPaths(structure?: FolderStructure): string[] {
  const s = structure || DEFAULT_FOLDER_STRUCTURE;
  const root = (s.manuscript || 'Manuscript').trim() || 'Manuscript';
  const acts = (Array.isArray(s.acts) && s.acts.length > 0)
    ? s.acts
    : DEFAULT_FOLDER_STRUCTURE.acts;
  return acts
    .map(a => a.trim())
    .filter(Boolean)
    .map(act => `${root}/${act}`);
}

function loadAssignments(): ModelAssignments {
  const stored = localStorage.getItem('modelAssignments');
  const defaults: ModelAssignments = {
    default: '', summaries: '', genre: '', keywords: '', continuity: '', showDontTell: '', aiIsms: '', prose: ''
  };
  if (stored) {
    try { return { ...defaults, ...JSON.parse(stored) }; } catch { /* use defaults */ }
  }
  const oldModel = localStorage.getItem('model') || '';
  const oldProse = localStorage.getItem('proseModel') || '';
  if (oldModel || oldProse) {
    defaults.default = oldModel;
    defaults.prose = oldProse;
  }
  return defaults;
}

export type ThemeMode = 'dark' | 'light';

const THEME_KEY = 'theme';

function applyTheme(mode: ThemeMode): void {
  document.documentElement.setAttribute('data-theme', mode);
  document.body.setAttribute('data-theme', mode);
}

function readStoredTheme(): ThemeMode {
  const stored = localStorage.getItem(THEME_KEY);
  if (stored === 'dark' || stored === 'light') return stored;
  return 'light';
}

const theme = ref<ThemeMode>(readStoredTheme());
applyTheme(theme.value);

async function saveThemeToServer(mode: ThemeMode): Promise<void> {
  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    const bypass = getOperatorBypassToken();
    if (bypass) {
      headers['x-loremetry-admin-bypass'] = bypass;
    }
    const authHeaders = await buildAuthHeaders();
    authHeaders.forEach((v, k) => { headers[k] = v; });
    if (!bypass && !headers.Authorization) return;

    await fetch('/api/me/preferences', {
      method: 'PATCH',
      headers,
      body: JSON.stringify({ theme: mode }),
    });
  } catch {
    /* localStorage still holds preference */
  }
}

function setTheme(mode: ThemeMode, opts?: { skipServer?: boolean }): void {
  theme.value = mode;
  localStorage.setItem(THEME_KEY, mode);
  applyTheme(mode);
  if (!opts?.skipServer) {
    void saveThemeToServer(mode);
  }
}

/** Apply account theme after session restore (server wins over stale localStorage). */
export function hydrateThemeFromAccount(accountTheme: string | undefined | null): void {
  if (accountTheme === 'light' || accountTheme === 'dark') {
    setTheme(accountTheme, { skipServer: true });
  }
}

const provider = ref(localStorage.getItem('provider') || 'tokenmix');
const modelAssignments = ref<ModelAssignments>(loadAssignments());
const models = ref<ModelInfo[]>(loadModelsFromStorage());
const folderStructure = ref<FolderStructure>({ ...DEFAULT_FOLDER_STRUCTURE, acts: [...DEFAULT_FOLDER_STRUCTURE.acts], extra: [...DEFAULT_FOLDER_STRUCTURE.extra] });

function loadModelsFromStorage(): ModelInfo[] {
  const stored = localStorage.getItem('cachedModels');
  if (stored) {
    try { return JSON.parse(stored); } catch { /* ignore */ }
  }
  return [];
}

/** Resolve the model for a given function. Falls back to default if unset. */
function modelFor(fn: keyof ModelAssignments): string {
  return modelAssignments.value[fn] || modelAssignments.value.default;
}

const model = computed(() => modelAssignments.value.default);
const proseModel = computed(() => modelAssignments.value.prose || modelAssignments.value.default);

async function fetchModels(): Promise<{ success: boolean; error: string }> {
  try {
    const result = await invoke<ModelsResult>('list_models', {
      provider: provider.value,
    });
    if (result.success && result.models.length > 0) {
      models.value = result.models;
      localStorage.setItem('cachedModels', JSON.stringify(result.models));
      return { success: true, error: '' };
    }
    return { success: false, error: result.error || 'No models returned. Configure platform API keys in Admin.' };
  } catch (e) {
    return { success: false, error: 'Error: ' + String(e) };
  }
}

async function saveSettings(): Promise<void> {
  localStorage.setItem(THEME_KEY, theme.value);
  localStorage.setItem('provider', provider.value);
  localStorage.setItem('modelAssignments', JSON.stringify(modelAssignments.value));
  localStorage.setItem('model', modelAssignments.value.default);
  localStorage.setItem('proseModel', modelAssignments.value.prose);
  void saveThemeToServer(theme.value);
}

async function testCanopy(): Promise<{ success: boolean; error: string }> {
  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    const bypass = getOperatorBypassToken();
    if (bypass) headers['x-loremetry-admin-bypass'] = bypass;
    const authHeaders = await buildAuthHeaders();
    authHeaders.forEach((v, k) => { headers[k] = v; });
    const res = await fetch('/api/settings/test-canopy', { method: 'POST', headers });
    if (!res.ok) {
      return { success: false, error: `HTTP ${res.status}` };
    }
    const data = await res.json() as { success?: boolean; error?: string };
    return { success: Boolean(data.success), error: data.error || '' };
  } catch (e) {
    return { success: false, error: String(e) };
  }
}

async function testDataforseo(): Promise<{ success: boolean; error: string }> {
  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    const bypass = getOperatorBypassToken();
    if (bypass) headers['x-loremetry-admin-bypass'] = bypass;
    const authHeaders = await buildAuthHeaders();
    authHeaders.forEach((v, k) => { headers[k] = v; });
    const res = await fetch('/api/settings/test-dataforseo', { method: 'POST', headers });
    if (!res.ok) {
      return { success: false, error: `HTTP ${res.status}` };
    }
    const data = await res.json() as { success?: boolean; error?: string };
    return { success: Boolean(data.success), error: data.error || '' };
  } catch (e) {
    return { success: false, error: String(e) };
  }
}

export type SetupIssue = { id: string; message: string };

function checkAiSetup(): SetupIssue[] {
  // Server auto-selects the cheapest model — no client-side check needed.
  return [];
}

function checkPublishAnalyzeSetup(): SetupIssue[] {
  return checkAiSetup();
}

function checkCraftAnalyzeSetup(): SetupIssue[] {
  return checkAiSetup();
}

function checkMarketIntelSetup(): SetupIssue[] {
  return checkAiSetup();
}

export function useSettings() {
  return {
    theme,
    setTheme,
    hydrateThemeFromAccount,
    provider,
    model,
    proseModel,
    modelAssignments,
    modelFor,
    models,
    folderStructure,
    fetchModels,
    saveSettings,
    testCanopy,
    testDataforseo,
    checkPublishAnalyzeSetup,
    checkCraftAnalyzeSetup,
    checkMarketIntelSetup,
  };
}
