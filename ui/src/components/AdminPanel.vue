<script setup lang="ts">
import { computed, inject, onMounted, ref } from 'vue';
import { adminFetch, adminUploadFile, getAdminToken, setAdminToken } from '../api';
import { settingsKey, showPanelKey } from '../injectionKeys';
import type { ModelInfo, StaleCleanupResult, WinningCatImportResult } from '../types';
import { useReportTypes } from '../composables/useReportTypes';

const showPanel = inject(showPanelKey)!;
const settingsCtx = inject(settingsKey)!;
const { reportTypes, loadReportTypes } = useReportTypes();
loadReportTypes();

const adminConfigured = ref(false);
const tokenInput = ref('');
const authenticated = ref(false);
const statusMsg = ref('');

const savedMsg = ref('');
const modelFetchStatus = ref('');
const canopyTestStatus = ref('');
const dataforseoTestStatus = ref('');

const winningcatStatus = ref('');
const staleStatus = ref('');
const showStaleRow = ref(false);
const importDisabled = ref(false);
let lastImportedAt = '';

type ModelSort = 'price' | 'provider';
const modelSort = ref<ModelSort>('price');

const sortedModels = computed(() => {
  return [...settingsCtx.models.value].sort((a, b) => {
    if (modelSort.value === 'provider') {
      const provA = a.owned_by.toLowerCase();
      const provB = b.owned_by.toLowerCase();
      if (provA !== provB) return provA.localeCompare(provB);
    }
    const priceA = a.input_price ?? Infinity;
    const priceB = b.input_price ?? Infinity;
    return priceA - priceB;
  });
});

type Tier = 'basic' | 'capable' | 'strong';
const TIER_RANK: Record<Tier, number> = { basic: 0, capable: 1, strong: 2 };

function modelTier(m: ModelInfo): Tier {
  const price = m.input_price ?? 0;
  if (price <= 0.001) return 'basic';
  if (price <= 0.01) return 'capable';
  return 'strong';
}

function minTierFor(fnKey: string): Tier {
  if (fnKey === 'prose') return 'strong';
  const tiers = reportTypes.value
    .filter(r => r.model_slot === fnKey)
    .map(r => (r.min_tier as Tier) || 'basic');
  if (tiers.length === 0) return 'basic';
  return tiers.reduce((best, t) => (TIER_RANK[t] > TIER_RANK[best] ? t : best), 'basic' as Tier);
}

function modelFitLabel(m: ModelInfo, fnKey: string): string {
  const tier = modelTier(m);
  const min = minTierFor(fnKey);
  if (TIER_RANK[tier] >= TIER_RANK[min]) return ' ✓';
  return ' ⚠';
}

function fnOptionLabel(m: ModelInfo, fnKey: string): string {
  return m.id + modelFitLabel(m, fnKey);
}

function modelLabel(m: ModelInfo): string {
  let label = m.id;
  if (m.owned_by) label += ` (${m.owned_by})`;
  if (m.input_price != null && m.output_price != null) {
    label += ` — $${m.input_price}/$${m.output_price} per 1K tokens`;
  }
  return label;
}

onMounted(async () => {
  try {
    const res = await fetch('/api/admin/status');
    const data = await res.json() as { configured?: boolean };
    adminConfigured.value = !!data.configured;
  } catch {
    adminConfigured.value = false;
  }
  const saved = getAdminToken();
  if (saved) {
    tokenInput.value = saved;
    authenticated.value = true;
  }
});

function onSaveToken(): void {
  const t = tokenInput.value.trim();
  if (!t) return;
  setAdminToken(t);
  authenticated.value = true;
  statusMsg.value = 'Token saved for this browser session.';
}

function onClearToken(): void {
  setAdminToken('');
  tokenInput.value = '';
  authenticated.value = false;
  statusMsg.value = 'Signed out of admin.';
}

async function onFetchModels(): Promise<void> {
  modelFetchStatus.value = 'Fetching models...';
  const result = await settingsCtx.fetchModels();
  modelFetchStatus.value = result.success
    ? `${settingsCtx.models.value.length} models loaded.`
    : result.error;
}

function onSave(): void {
  settingsCtx.saveSettings().then(() => {
    savedMsg.value = '✓ Saved';
    setTimeout(() => { savedMsg.value = ''; }, 2000);
  }).catch((e) => {
    savedMsg.value = 'Save failed: ' + String(e);
  });
}

async function onTestCanopy(): Promise<void> {
  canopyTestStatus.value = 'Testing...';
  const result = await settingsCtx.testCanopy();
  canopyTestStatus.value = result.success ? '✓ Connected' : '✗ ' + result.error;
}

async function onTestDataforseo(): Promise<void> {
  dataforseoTestStatus.value = 'Testing...';
  const result = await settingsCtx.testDataforseo();
  dataforseoTestStatus.value = result.success ? '✓ Connected' : '✗ ' + result.error;
}

async function onWinningCatFile(ev: Event): Promise<void> {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;

  winningcatStatus.value = 'Importing...';
  importDisabled.value = true;
  showStaleRow.value = false;
  try {
    const result = await adminUploadFile<WinningCatImportResult>('/winningcat/upload', file);
    if (result.success) {
      winningcatStatus.value = `✓ Imported ${result.imported} categories. Skipped ${result.skipped_other_department} (other department), ${result.skipped_unparseable} (unparseable).`;
      lastImportedAt = result.imported_at;
      if (result.stale_count > 0) {
        showStaleRow.value = true;
        const word = result.stale_count === 1 ? 'y was' : 'ies were';
        staleStatus.value = `${result.stale_count} categor${word} in the catalog from a previous import but missing from this one.`;
      }
    } else {
      winningcatStatus.value = result.error || 'Import failed.';
    }
  } catch (e) {
    winningcatStatus.value = 'Error: ' + String(e);
  } finally {
    importDisabled.value = false;
  }
}

async function onRemoveStale(): Promise<void> {
  if (!lastImportedAt) return;
  if (!confirm('Remove stale categories from the catalog? Only reference data is affected.')) return;
  try {
    const result = await adminFetch<StaleCleanupResult>('/winningcat/remove-stale', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ since: lastImportedAt }),
    });
    if (result.success) {
      const word = result.removed === 1 ? 'y' : 'ies';
      staleStatus.value = `✓ Removed ${result.removed} stale categor${word}.`;
      showStaleRow.value = false;
    } else {
      staleStatus.value = result.error || 'Cleanup failed.';
    }
  } catch (e) {
    staleStatus.value = 'Error: ' + String(e);
  }
}
</script>

<template>
  <div class="admin-panel">
    <div class="panel-header">
      <h2 class="panel-title">Admin</h2>
      <button type="button" class="btn btn-sm" @click="showPanel('analyzer')">← Back</button>
    </div>

    <p class="panel-desc">
      Operator configuration — API keys, models, and catalog import. Saved in this browser; leave keys blank to use server env vars on Miget.
    </p>

    <div v-if="!adminConfigured" class="admin-notice">
      Set <code>ADMIN_TOKEN</code> in Miget environment variables, then redeploy.
    </div>

    <template v-else>
      <div v-if="!authenticated" class="settings-form">
        <h3 class="section-title">Sign in</h3>
        <label class="field-label">Admin token</label>
        <input
          v-model="tokenInput"
          type="password"
          placeholder="Same value as ADMIN_TOKEN on the server"
          autocomplete="off"
        />
        <button type="button" class="btn btn-sm" @click="onSaveToken">Continue</button>
        <div class="status-msg">{{ statusMsg }}</div>
      </div>

      <template v-else>
        <div class="token-row">
          <span class="token-ok">✓ Admin signed in</span>
          <button type="button" class="btn btn-sm" @click="onClearToken">Sign out</button>
        </div>

        <!-- Appearance -->
        <div class="settings-section-divider"></div>
        <h3 class="section-title">Appearance</h3>
        <div class="settings-form">
          <label class="field-label">Theme</label>
          <div class="provider-options">
            <label class="provider-option" :class="{ active: settingsCtx.theme.value === 'dark' }">
              <input
                type="radio"
                name="theme"
                value="dark"
                :checked="settingsCtx.theme.value === 'dark'"
                @change="settingsCtx.setTheme('dark')"
              />
              Dark
            </label>
            <label class="provider-option" :class="{ active: settingsCtx.theme.value === 'light' }">
              <input
                type="radio"
                name="theme"
                value="light"
                :checked="settingsCtx.theme.value === 'light'"
                @change="settingsCtx.setTheme('light')"
              />
              Light
            </label>
          </div>
        </div>

        <!-- AI provider -->
        <div class="settings-section-divider"></div>
        <h3 class="section-title">AI provider</h3>
        <div class="settings-form">
          <label class="field-label">Provider</label>
          <div class="provider-options">
            <label class="provider-option">
              <input type="radio" v-model="settingsCtx.provider.value" value="claude" />
              Claude
            </label>
            <label class="provider-option">
              <input type="radio" v-model="settingsCtx.provider.value" value="tokenmix" />
              TokenMix
            </label>
          </div>

          <label class="field-label">API key</label>
          <input
            type="password"
            v-model="settingsCtx.apiKey.value"
            placeholder="Optional if set on server (ANTHROPIC_API_KEY / TOKENMIX_API_KEY)"
          />

          <label class="field-label">
            Default model
            <span class="model-hint">Fetch models first, then assign each function below.</span>
          </label>
          <div class="model-row">
            <select v-model="settingsCtx.modelAssignments.value.default">
              <option v-if="sortedModels.length === 0" value="" disabled>No models loaded</option>
              <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ modelLabel(m) }}</option>
            </select>
            <button type="button" class="btn btn-sm" @click="onFetchModels">Fetch models</button>
          </div>
          <div class="status-msg">{{ modelFetchStatus }}</div>

          <div v-if="sortedModels.length > 0" class="model-sort-row">
            <span class="model-sort-label">Sort:</span>
            <button type="button" class="model-sort-btn" :class="{ active: modelSort === 'price' }" @click="modelSort = 'price'">Price</button>
            <button type="button" class="model-sort-btn" :class="{ active: modelSort === 'provider' }" @click="modelSort = 'provider'">Provider</button>
          </div>

          <div v-if="sortedModels.length > 0" class="model-assignments">
            <div class="model-assign-header">Model per function</div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Chapter summaries</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.summaries">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'summaries') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Genre analysis</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.genre">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'genre') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Keywords &amp; categories</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.keywords">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'keywords') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Continuity</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.continuity">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'continuity') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Show don't tell</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.showDontTell">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'showDontTell') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>AI-isms</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.aiIsms">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'aiIsms') }}</option>
              </select>
            </div>
            <div class="model-assign-row">
              <div class="model-assign-label"><strong>Prose suggestions</strong></div>
              <select v-model="settingsCtx.modelAssignments.value.prose">
                <option value="">(Use default)</option>
                <option v-for="m in sortedModels" :key="m.id" :value="m.id">{{ fnOptionLabel(m, 'prose') }}</option>
              </select>
            </div>
          </div>

          <button type="button" class="btn" @click="onSave">Save settings</button>
          <div class="settings-saved">{{ savedMsg }}</div>
        </div>

        <!-- Canopy -->
        <div class="settings-section-divider"></div>
        <h3 class="section-title">Canopy API</h3>
        <div class="settings-form">
          <label class="field-label">Canopy API key</label>
          <input type="password" v-model="settingsCtx.canopyApiKey.value" placeholder="Optional if CANOPY_API_KEY is set on server" />
          <button type="button" class="btn btn-sm" @click="onTestCanopy">Test connection</button>
          <div class="status-msg">{{ canopyTestStatus }}</div>
        </div>

        <!-- DataForSEO -->
        <div class="settings-section-divider"></div>
        <h3 class="section-title">DataForSEO</h3>
        <div class="settings-form">
          <p class="panel-desc">Keyword search volume (Amazon + Google).</p>
          <label class="field-label">Login</label>
          <input type="text" v-model="settingsCtx.dataforseoLogin.value" placeholder="your@email.com" />
          <label class="field-label">Password</label>
          <input type="password" v-model="settingsCtx.dataforseoPassword.value" placeholder="DataForSEO API password" />
          <button type="button" class="btn btn-sm" @click="onTestDataforseo">Test connection</button>
          <div class="status-msg">{{ dataforseoTestStatus }}</div>
        </div>

        <!-- WinningCat -->
        <div class="settings-section-divider"></div>
        <h3 class="section-title">WinningCat catalog</h3>
        <div class="settings-form">
          <p class="panel-desc">Import browse-node CSV into the server database.</p>
          <label class="btn file-btn" :class="{ disabled: importDisabled }">
            Import CSV
            <input type="file" accept=".csv,text/csv" :disabled="importDisabled" hidden @change="onWinningCatFile" />
          </label>
          <div class="status-msg">{{ winningcatStatus }}</div>
          <div v-if="showStaleRow" class="stale-row">
            <div class="status-msg">{{ staleStatus }}</div>
            <button type="button" class="btn btn-sm btn-danger" @click="onRemoveStale">Remove stale</button>
          </div>
        </div>
      </template>
    </template>
  </div>
</template>

<style scoped>
.admin-panel {
  padding: 20px;
  overflow-y: auto;
  max-width: 560px;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
}

.panel-desc {
  font-size: 13px;
  color: var(--text-muted);
  margin-bottom: 16px;
  line-height: 1.5;
}

.admin-notice {
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface2);
  font-size: 13px;
  line-height: 1.5;
}

.admin-notice code {
  font-family: var(--mono);
  font-size: 12px;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 10px;
}

.settings-section-divider {
  border-top: 1px solid var(--border);
  margin: 20px 0;
}

.settings-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.field-label {
  font-size: 12px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.settings-form input,
.settings-form select {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  font-size: 13px;
  padding: 8px 10px;
  width: 100%;
  user-select: text;
}

.settings-form select option {
  background: var(--surface2);
}

.provider-options {
  display: flex;
  gap: 16px;
}

.provider-option {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text);
  text-transform: none;
  letter-spacing: 0;
  cursor: pointer;
}

.provider-option input[type="radio"] {
  width: auto;
  accent-color: var(--accent);
}

.model-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.model-row select {
  flex: 1;
}

.model-hint {
  display: block;
  font-size: 11px;
  color: var(--text-muted);
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
  margin-top: 2px;
}

.status-msg,
.settings-saved {
  font-size: 12px;
  color: var(--text-muted);
  min-height: 16px;
}

.settings-saved {
  color: var(--success);
}

.model-sort-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.model-sort-label {
  font-size: 11px;
  color: var(--text-muted);
}

.model-sort-btn {
  background: none;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 11px;
  padding: 3px 8px;
  cursor: pointer;
}

.model-sort-btn.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.model-assignments {
  margin-top: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
}

.model-assign-header {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-muted);
  margin-bottom: 10px;
}

.model-assign-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}

.model-assign-row:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.model-assign-label strong {
  font-size: 13px;
  color: var(--text);
}

.token-row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
}

.token-ok {
  color: var(--success);
}

.file-btn {
  display: inline-block;
  width: fit-content;
  cursor: pointer;
}

.file-btn.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.stale-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.btn {
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: #fff;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
  padding: 9px 18px;
  align-self: flex-start;
}

.btn:hover {
  background: var(--accent-dim);
}

.btn-sm {
  padding: 6px 12px;
  font-size: 12px;
}

.btn-danger {
  background: var(--danger);
  align-self: flex-start;
}
</style>
