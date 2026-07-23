import { ref, computed } from 'vue';
import { invoke } from '../api';
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

function applyTheme(mode: ThemeMode): void {
  document.documentElement.setAttribute('data-theme', mode);
}

const theme = ref<ThemeMode>(
  (localStorage.getItem('theme') as ThemeMode) === 'light' ? 'light' : 'dark'
);
applyTheme(theme.value);

function setTheme(mode: ThemeMode): void {
  theme.value = mode;
  localStorage.setItem('theme', mode);
  applyTheme(mode);
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
  localStorage.setItem('theme', theme.value);
  localStorage.setItem('provider', provider.value);
  localStorage.setItem('modelAssignments', JSON.stringify(modelAssignments.value));
  localStorage.setItem('model', modelAssignments.value.default);
  localStorage.setItem('proseModel', modelAssignments.value.prose);
}

export function useSettings() {
  return {
    theme,
    setTheme,
    provider,
    model,
    proseModel,
    modelAssignments,
    modelFor,
    models,
    folderStructure,
    fetchModels,
    saveSettings,
  };
}
