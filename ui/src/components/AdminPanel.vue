<script setup lang="ts">
import { computed, inject, onMounted, ref } from 'vue';
import { adminFetch, adminUploadFile } from '../api';
import { settingsKey, showPanelKey } from '../injectionKeys';
import type { ModelInfo, StaleCleanupResult, WinningCatImportResult } from '../types';
import { useReportTypes } from '../composables/useReportTypes';

const showPanel = inject(showPanelKey)!;
const settingsCtx = inject(settingsKey)!;
const { reportTypes, loadReportTypes } = useReportTypes();
loadReportTypes();

const savedMsg = ref('');
const modelFetchStatus = ref('');

const platformStatus = ref<{
  anthropic: boolean;
  tokenmix: boolean;
  canopy: boolean;
  dataforseo: boolean;
  default_provider: string;
} | null>(null);
const platformSaveMsg = ref('');
const platformCanopyStatus = ref('');
const platformDfsStatus = ref('');
const credAnthropic = ref('');
const credTokenmix = ref('');
const credCanopy = ref('');
const credDfsLogin = ref('');
const credDfsPassword = ref('');
const credDefaultProvider = ref('tokenmix');

type UsageSummaryRow = {
  user_id: string;
  email: string;
  role: string;
  monthly_fee_cents: number;
  total_cost_usd: number;
  input_tokens: number;
  output_tokens: number;
};
const usageMonth = ref(monthInputValue());
const usageRows = ref<UsageSummaryRow[]>([]);
const usageError = ref('');
const usageLoading = ref(false);

const winningcatStatus = ref('');
const staleStatus = ref('');
const showStaleRow = ref(false);
const importDisabled = ref(false);
let lastImportedAt = '';

type DbTable = { schema: string; name: string; qualified: string };
const dbTables = ref<DbTable[]>([]);
const sqlQuery = ref('SELECT * FROM lore.stories LIMIT 50');
const sqlRunning = ref(false);
const sqlError = ref('');
const sqlMeta = ref('');
const sqlColumns = ref<string[]>([]);
const sqlRows = ref<unknown[][]>([]);

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

onMounted(() => {
  loadDbTables();
  loadPlatformSecrets();
  loadUsageSummary();
});

function monthInputValue(d = new Date()): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
}

function usageRangeFromMonth(ym: string): { from: string; to: string } {
  const [y, m] = ym.split('-').map(Number);
  const from = new Date(Date.UTC(y, m - 1, 1));
  const to = new Date(Date.UTC(y, m, 1));
  return { from: from.toISOString(), to: to.toISOString() };
}

async function loadPlatformSecrets(): Promise<void> {
  try {
    const status = await adminFetch<{
      anthropic: boolean;
      tokenmix: boolean;
      canopy: boolean;
      dataforseo: boolean;
      default_provider: string;
    }>('/platform-secrets');
    platformStatus.value = status;
    if (status.default_provider) {
      credDefaultProvider.value = status.default_provider;
    }
  } catch {
    platformStatus.value = null;
  }
}

async function savePlatformSecrets(): Promise<void> {
  platformSaveMsg.value = 'Saving…';
  const body: Record<string, string> = {
    default_provider: credDefaultProvider.value,
  };
  if (credAnthropic.value.trim()) body.anthropic_api_key = credAnthropic.value.trim();
  if (credTokenmix.value.trim()) body.tokenmix_api_key = credTokenmix.value.trim();
  if (credCanopy.value.trim()) body.canopy_api_key = credCanopy.value.trim();
  if (credDfsLogin.value.trim()) body.dataforseo_login = credDfsLogin.value.trim();
  if (credDfsPassword.value.trim()) body.dataforseo_password = credDfsPassword.value.trim();
  try {
    const result = await adminFetch<{ success: boolean; error?: string }>('/platform-secrets', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (result.success) {
      platformSaveMsg.value = '✓ Saved';
      credAnthropic.value = '';
      credTokenmix.value = '';
      credCanopy.value = '';
      credDfsLogin.value = '';
      credDfsPassword.value = '';
      await loadPlatformSecrets();
    } else {
      platformSaveMsg.value = result.error || 'Save failed';
    }
  } catch (e) {
    platformSaveMsg.value = String(e);
  }
  setTimeout(() => { platformSaveMsg.value = ''; }, 3000);
}

async function onTestPlatformCanopy(): Promise<void> {
  platformCanopyStatus.value = 'Testing…';
  try {
    const result = await adminFetch<{ success: boolean; error: string }>('/platform-secrets/test-canopy', {
      method: 'POST',
    });
    platformCanopyStatus.value = result.success ? '✓ Connected' : '✗ ' + result.error;
  } catch (e) {
    platformCanopyStatus.value = '✗ ' + String(e);
  }
}

async function onTestPlatformDataforseo(): Promise<void> {
  platformDfsStatus.value = 'Testing…';
  try {
    const result = await adminFetch<{ success: boolean; error: string }>('/platform-secrets/test-dataforseo', {
      method: 'POST',
    });
    platformDfsStatus.value = result.success ? '✓ Connected' : '✗ ' + result.error;
  } catch (e) {
    platformDfsStatus.value = '✗ ' + String(e);
  }
}

async function loadUsageSummary(): Promise<void> {
  usageLoading.value = true;
  usageError.value = '';
  const { from, to } = usageRangeFromMonth(usageMonth.value);
  try {
    const result = await adminFetch<{
      success: boolean;
      error?: string;
      users: UsageSummaryRow[];
    }>(`/usage/summary?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`);
    if (result.success) {
      usageRows.value = result.users ?? [];
    } else {
      usageRows.value = [];
      usageError.value = result.error || 'Failed to load usage';
    }
  } catch (e) {
    usageRows.value = [];
    usageError.value = String(e);
  } finally {
    usageLoading.value = false;
  }
}

const usageTotals = computed(() => {
  return usageRows.value.reduce(
    (acc, row) => {
      acc.cost += row.total_cost_usd;
      acc.input += row.input_tokens;
      acc.output += row.output_tokens;
      acc.fee += row.monthly_fee_cents;
      return acc;
    },
    { cost: 0, input: 0, output: 0, fee: 0 },
  );
});

function formatUsd(n: number): string {
  return '$' + n.toFixed(4);
}

function formatFeeCents(cents: number): string {
  return '$' + (cents / 100).toFixed(2);
}

function configuredLabel(ok: boolean): string {
  return ok ? '✓ configured' : '— not set';
}

async function loadDbTables(): Promise<void> {
  try {
    const data = await adminFetch<{ success?: boolean; tables?: DbTable[] }>('/tables');
    dbTables.value = data.tables ?? [];
  } catch {
    dbTables.value = [];
  }
}

function insertTableQuery(table: DbTable): void {
  sqlQuery.value = `SELECT * FROM ${table.qualified} LIMIT 50`;
}

async function onRunSql(): Promise<void> {
  const sql = sqlQuery.value.trim();
  if (!sql) return;
  sqlRunning.value = true;
  sqlError.value = '';
  sqlMeta.value = '';
  sqlColumns.value = [];
  sqlRows.value = [];
  try {
    const result = await adminFetch<{
      success: boolean;
      error?: string;
      columns?: string[];
      rows?: unknown[][];
      rows_affected?: number;
      duration_ms?: number;
    }>('/sql', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ sql }),
    });
    if (!result.success) {
      sqlError.value = result.error || 'Query failed';
      return;
    }
    sqlColumns.value = result.columns ?? [];
    sqlRows.value = result.rows ?? [];
    const affected = result.rows_affected ?? 0;
    const ms = result.duration_ms ?? 0;
    sqlMeta.value = `${affected} row${affected === 1 ? '' : 's'} · ${ms} ms`;
  } catch (e) {
    sqlError.value = String(e);
  } finally {
    sqlRunning.value = false;
  }
}

function formatCell(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
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
      Operator tools — platform API credentials (encrypted on server), usage reporting, catalog import, and SQL console.
    </p>

    <!-- SQL console -->
    <h3 class="section-title">SQL console</h3>
    <div class="settings-form sql-section">
      <p class="panel-desc">Run queries against the Postgres database. App data lives in the <code>lore</code> schema.</p>
      <div v-if="dbTables.length > 0" class="table-picker">
        <span class="table-picker-label">Tables</span>
        <div class="table-chips">
          <button
            v-for="t in dbTables"
            :key="t.qualified"
            type="button"
            class="table-chip"
            :title="t.qualified"
            @click="insertTableQuery(t)"
          >
            {{ t.qualified }}
          </button>
        </div>
      </div>
      <textarea
        v-model="sqlQuery"
        class="sql-input"
        rows="6"
        spellcheck="false"
        placeholder="SELECT * FROM lore.stories LIMIT 50"
        @keydown.meta.enter.prevent="onRunSql"
        @keydown.ctrl.enter.prevent="onRunSql"
      />
      <div class="sql-actions">
        <button type="button" class="btn btn-sm" :disabled="sqlRunning" @click="onRunSql">
          {{ sqlRunning ? 'Running…' : 'Run (⌘↵)' }}
        </button>
        <span v-if="sqlMeta" class="status-msg">{{ sqlMeta }}</span>
      </div>
      <div v-if="sqlError" class="sql-error">{{ sqlError }}</div>
      <div v-if="sqlColumns.length > 0" class="sql-results-wrap">
        <table class="sql-results">
          <thead>
            <tr>
              <th v-for="col in sqlColumns" :key="col">{{ col }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, ri) in sqlRows" :key="ri">
              <td v-for="(cell, ci) in row" :key="ci">{{ formatCell(cell) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="settings-section-divider"></div>

    <h3 class="section-title">Platform credentials</h3>
    <div class="settings-form">
      <p class="panel-desc">Stored encrypted on the server. Leave a field blank to keep the current value.</p>
      <div v-if="platformStatus" class="platform-status">
        <span>Anthropic: {{ configuredLabel(platformStatus.anthropic) }}</span>
        <span>TokenMix: {{ configuredLabel(platformStatus.tokenmix) }}</span>
        <span>Canopy: {{ configuredLabel(platformStatus.canopy) }}</span>
        <span>DataForSEO: {{ configuredLabel(platformStatus.dataforseo) }}</span>
      </div>
      <label class="field-label">Anthropic API key</label>
      <input v-model="credAnthropic" type="password" autocomplete="off" placeholder="sk-ant-…" />
      <label class="field-label">TokenMix API key</label>
      <input v-model="credTokenmix" type="password" autocomplete="off" placeholder="tm-…" />
      <label class="field-label">Canopy API key</label>
      <input v-model="credCanopy" type="password" autocomplete="off" />
      <button type="button" class="btn btn-sm" @click="onTestPlatformCanopy">Test Canopy</button>
      <div class="status-msg">{{ platformCanopyStatus }}</div>
      <label class="field-label">DataForSEO login</label>
      <input v-model="credDfsLogin" type="text" autocomplete="off" />
      <label class="field-label">DataForSEO password</label>
      <input v-model="credDfsPassword" type="password" autocomplete="off" />
      <button type="button" class="btn btn-sm" @click="onTestPlatformDataforseo">Test DataForSEO</button>
      <div class="status-msg">{{ platformDfsStatus }}</div>
      <label class="field-label">Default LLM provider</label>
      <div class="provider-options">
        <label class="provider-option">
          <input v-model="credDefaultProvider" type="radio" value="claude" />
          Claude
        </label>
        <label class="provider-option">
          <input v-model="credDefaultProvider" type="radio" value="tokenmix" />
          TokenMix
        </label>
      </div>
      <button type="button" class="btn" @click="savePlatformSecrets">Save platform credentials</button>
      <div class="settings-saved">{{ platformSaveMsg }}</div>
    </div>

    <div class="settings-section-divider"></div>
    <h3 class="section-title">Usage &amp; cost</h3>
    <div class="settings-form usage-toolbar">
      <label class="field-label">Month</label>
      <input v-model="usageMonth" type="month" @change="loadUsageSummary" />
      <button type="button" class="btn btn-sm" :disabled="usageLoading" @click="loadUsageSummary">
        {{ usageLoading ? 'Loading…' : 'Refresh' }}
      </button>
      <div v-if="usageError" class="sql-error">{{ usageError }}</div>
      <table v-if="usageRows.length > 0" class="sql-results usage-table">
        <thead>
          <tr>
            <th>User</th>
            <th>Role</th>
            <th>Monthly fee</th>
            <th>AI cost</th>
            <th>Input tokens</th>
            <th>Output tokens</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in usageRows" :key="row.user_id">
            <td>{{ row.email }}</td>
            <td>{{ row.role }}</td>
            <td>{{ formatFeeCents(row.monthly_fee_cents) }}</td>
            <td>{{ formatUsd(row.total_cost_usd) }}</td>
            <td>{{ row.input_tokens.toLocaleString() }}</td>
            <td>{{ row.output_tokens.toLocaleString() }}</td>
          </tr>
        </tbody>
        <tfoot>
          <tr>
            <td colspan="2"><strong>Totals</strong></td>
            <td>{{ formatFeeCents(usageTotals.fee) }}</td>
            <td>{{ formatUsd(usageTotals.cost) }}</td>
            <td>{{ usageTotals.input.toLocaleString() }}</td>
            <td>{{ usageTotals.output.toLocaleString() }}</td>
          </tr>
        </tfoot>
      </table>
      <p v-else-if="!usageLoading" class="panel-desc">No usage in this period.</p>
    </div>

    <div class="settings-section-divider"></div>

    <!-- Appearance -->
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
        <h3 class="section-title">AI provider &amp; models</h3>
        <div class="settings-form">
          <label class="field-label">Provider (this browser)</label>
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
  </div>
</template>

<style scoped>
.platform-status {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 12px;
}

.usage-table {
  margin-top: 12px;
}

.usage-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}

.admin-panel {
  padding: 20px;
  overflow-y: auto;
  max-width: 960px;
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

.panel-desc code {
  font-family: var(--mono);
  font-size: 12px;
}

.sql-section {
  margin-bottom: 4px;
}

.sql-input {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.5;
  padding: 10px;
  resize: vertical;
  width: 100%;
  min-height: 120px;
}

.sql-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.sql-error {
  color: var(--danger);
  font-size: 12px;
  font-family: var(--mono);
  white-space: pre-wrap;
}

.sql-results-wrap {
  overflow-x: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  max-height: 420px;
  overflow-y: auto;
}

.sql-results {
  border-collapse: collapse;
  font-size: 12px;
  width: max-content;
  min-width: 100%;
}

.sql-results th,
.sql-results td {
  border-bottom: 1px solid var(--border);
  padding: 6px 10px;
  text-align: left;
  vertical-align: top;
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sql-results th {
  background: var(--surface2);
  font-weight: 600;
  position: sticky;
  top: 0;
}

.sql-results td {
  font-family: var(--mono);
}

.table-picker {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.table-picker-label {
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.table-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.table-chip {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  cursor: pointer;
  font-family: var(--mono);
  font-size: 11px;
  padding: 4px 8px;
}

.table-chip:hover {
  border-color: var(--accent);
  color: var(--text);
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
