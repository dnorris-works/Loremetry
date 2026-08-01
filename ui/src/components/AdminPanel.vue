<script setup lang="ts">
import { computed, inject, onMounted, ref } from 'vue';
import { adminFetch, adminUploadFile, invoke } from '../api';
import { showPanelKey } from '../injectionKeys';
import type { StaleCleanupResult, WinningCatCatalogStatus, WinningCatImportResult } from '../types';

const showPanel = inject(showPanelKey)!;

type PlatformSecretsView = {
  anthropic_configured: boolean;
  tokenmix_configured: boolean;
  canopy_configured: boolean;
  dataforseo_configured: boolean;
  default_provider: string;
  clerk_configured: boolean;
  admin_bypass_configured: boolean;
  bootstrap_email_configured: boolean;
  env_managed_message: string;
};

const platformStatus = ref<PlatformSecretsView | null>(null);
const platformCanopyStatus = ref('');
const platformDfsStatus = ref('');

const serviceStatuses = computed(() => {
  if (!platformStatus.value) return [];
  return [
    { key: 'tokenmix', label: 'TokenMix AI', configured: platformStatus.value.tokenmix_configured },
    { key: 'anthropic', label: 'Anthropic', configured: platformStatus.value.anthropic_configured },
    { key: 'canopy', label: 'Canopy', configured: platformStatus.value.canopy_configured },
    { key: 'dataforseo', label: 'DataForSEO', configured: platformStatus.value.dataforseo_configured },
    { key: 'clerk', label: 'Clerk Auth', configured: platformStatus.value.clerk_configured },
    { key: 'bypass', label: 'Admin Bypass', configured: platformStatus.value.admin_bypass_configured },
    { key: 'email', label: 'Bootstrap Email', configured: platformStatus.value.bootstrap_email_configured },
  ];
});

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
const catalogChecking = ref(false);
const catalogStatus = ref<WinningCatCatalogStatus | null>(null);
let lastImportedAt = '';

function catalogStatusTitle(): string {
  const s = catalogStatus.value;
  if (!s || !s.success) return 'Check failed';
  if (s.ready) return 'Catalog ready';
  if (s.has_data) return 'Partial catalog';
  return 'Not imported';
}

function catalogStatusClass(): string {
  const s = catalogStatus.value;
  if (!s || !s.success) return 'catalog-status-error';
  if (s.ready) return 'catalog-status-ready';
  if (s.has_data) return 'catalog-status-partial';
  return 'catalog-status-empty';
}

async function refreshCatalogStatus(): Promise<void> {
  catalogChecking.value = true;
  try {
    catalogStatus.value = await adminFetch<WinningCatCatalogStatus>('/winningcat/status');
  } catch (e) {
    catalogStatus.value = {
      success: false,
      has_data: false,
      ready: false,
      kindle_count: 0,
      books_count: 0,
      total_count: 0,
      last_import_at: '',
      message: '',
      error: String(e),
    };
  } finally {
    catalogChecking.value = false;
  }
}

type DbTable = { schema: string; name: string; qualified: string };
const dbTables = ref<DbTable[]>([]);
const dbTablesError = ref('');
const sqlQuery = ref('SELECT * FROM lore.stories LIMIT 50');
const sqlRunning = ref(false);
const sqlError = ref('');
const sqlMeta = ref('');
const sqlColumns = ref<string[]>([]);
const sqlRows = ref<unknown[][]>([]);

onMounted(() => {
  loadDbTables();
  loadPlatformSecrets();
  loadUsageSummary();
  refreshCatalogStatus();
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
    const data = await invoke<PlatformSecretsView>('get_platform_credentials');
    platformStatus.value = data;
  } catch {
    try {
      const data = await adminFetch<PlatformSecretsView>('/platform-secrets');
      platformStatus.value = data;
    } catch {
      platformStatus.value = null;
    }
  }
}

async function onTestCanopy(): Promise<void> {
  platformCanopyStatus.value = 'Testing…';
  try {
    const result = await invoke<{ success: boolean; error: string }>('test_canopy_connection', {});
    platformCanopyStatus.value = result.success ? '✓ Connected' : '✗ ' + (result.error || 'Connection failed');
  } catch (e) {
    platformCanopyStatus.value = '✗ ' + String(e);
  }
}

async function onTestDfs(): Promise<void> {
  platformDfsStatus.value = 'Testing…';
  try {
    const result = await invoke<{ success: boolean; error: string }>('test_dataforseo_connection', {});
    platformDfsStatus.value = result.success ? '✓ Connected' : '✗ ' + (result.error || 'Connection failed');
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

async function loadDbTables(): Promise<void> {
  dbTablesError.value = '';
  try {
    const data = await adminFetch<{ success?: boolean; tables?: DbTable[]; error?: string }>('/tables');
    if (data.success === false) {
      dbTables.value = [];
      dbTablesError.value = data.error ?? 'Could not load table list';
      return;
    }
    dbTables.value = data.tables ?? [];
  } catch (e) {
    dbTables.value = [];
    dbTablesError.value = String(e);
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
      await refreshCatalogStatus();
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
      await refreshCatalogStatus();
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
      Operator tools — platform service status, usage reporting, catalog import, and SQL console.
    </p>

    <!-- SQL console -->
    <h3 class="section-title">SQL console</h3>
    <div class="settings-form sql-section">
      <p class="panel-desc">Run queries against the Postgres database. App data lives in the <code>lore</code> schema (including lookup/config tables such as <code>lookup_config</code>, <code>provider_models</code>, <code>report_types</code>, <code>genres</code>, <code>kdp_categories</code>).</p>
      <div v-if="dbTablesError" class="sql-error">{{ dbTablesError }}</div>
      <div v-if="dbTables.length > 0" class="table-picker">
        <div class="table-picker-header">
          <span class="table-picker-label">Tables ({{ dbTables.length }})</span>
          <button type="button" class="btn btn-sm" @click="loadDbTables">Refresh</button>
        </div>
        <div class="table-list-wrap">
          <div class="table-name-grid">
            <button
              v-for="t in dbTables"
              :key="t.qualified"
              type="button"
              class="table-name-btn"
              :title="t.qualified"
              @click="insertTableQuery(t)"
            >
              {{ t.name }}
            </button>
          </div>
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
      <p class="env-managed-notice">Credentials managed via Miget environment variables.</p>

      <div v-if="platformStatus" class="service-status-grid">
        <div class="service-row" v-for="svc in serviceStatuses" :key="svc.key">
          <span class="service-name">{{ svc.label }}</span>
          <span :class="svc.configured ? 'status-ok' : 'status-missing'">
            {{ svc.configured ? '✓ Configured' : '— Not configured' }}
          </span>
        </div>
        <div class="service-row">
          <span class="service-name">Default provider</span>
          <span class="status-ok">{{ platformStatus.default_provider }}</span>
        </div>
      </div>

      <div class="test-connections" v-if="platformStatus">
        <button type="button" class="btn btn-sm" @click="onTestCanopy" :disabled="!platformStatus.canopy_configured">
          Test Canopy
        </button>
        <span v-if="platformCanopyStatus" class="status-msg">{{ platformCanopyStatus }}</span>

        <button type="button" class="btn btn-sm" @click="onTestDfs" :disabled="!platformStatus.dataforseo_configured">
          Test DataForSEO
        </button>
        <span v-if="platformDfsStatus" class="status-msg">{{ platformDfsStatus }}</span>
      </div>
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
    <h3 class="section-title">WinningCat catalog</h3>
        <div class="settings-form">
          <p class="panel-desc">Import browse-node CSV into the server database (Kindle Store and Books departments).</p>
          <div class="catalog-status-block">
            <button type="button" class="btn btn-sm" :disabled="catalogChecking" @click="refreshCatalogStatus">
              {{ catalogChecking ? 'Checking…' : 'Check catalog' }}
            </button>
            <div
              v-if="catalogStatus"
              class="catalog-status"
              :class="catalogStatusClass()"
            >
              <div class="catalog-status-title">{{ catalogStatusTitle() }}</div>
              <div class="catalog-status-body">
                {{ catalogStatus.success ? catalogStatus.message : (catalogStatus.error || 'Could not read catalog status from the database.') }}
              </div>
            </div>
          </div>
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
.env-managed-notice {
  font-size: 13px;
  color: var(--text-muted);
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 14px;
  margin-bottom: 16px;
  line-height: 1.5;
}

.service-status-grid {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 16px;
}

.service-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-radius: var(--radius);
  background: var(--surface2);
}

.service-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
}

.status-ok {
  font-size: 12px;
  color: var(--success);
  font-weight: 500;
}

.status-missing {
  font-size: 12px;
  color: var(--text-muted);
}

.test-connections {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
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
  padding: var(--content-pad, 20px) 24px;
  overflow-y: auto;
  width: 100%;
  max-width: none;
  min-width: 0;
  box-sizing: border-box;
  align-self: stretch;
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

.table-list-wrap {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 8px 10px;
  overflow: hidden;
}

.table-name-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 8px 20px;
  width: 100%;
  min-width: 0;
}

.table-name-btn {
  margin: 0;
  padding: 6px 10px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--text);
  font-family: var(--mono);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  min-width: 0;
  white-space: nowrap;
}

.table-name-btn:hover {
  background: var(--surface2);
}

.table-name-btn:focus-visible {
  outline: 1px solid var(--accent);
  outline-offset: 1px;
}

.table-picker-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.table-picker-label {
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
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
  width: 100%;
  max-width: none;
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

.catalog-status-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 12px;
}

.catalog-status {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  font-size: 13px;
}

.catalog-status-title {
  font-weight: 600;
  margin-bottom: 4px;
}

.catalog-status-body {
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.45;
}

.catalog-status-ready {
  border-color: var(--success);
  background: color-mix(in srgb, var(--success) 8%, transparent);
}

.catalog-status-ready .catalog-status-title {
  color: var(--success);
}

.catalog-status-partial {
  border-color: var(--warning);
  background: color-mix(in srgb, var(--warning) 8%, transparent);
}

.catalog-status-partial .catalog-status-title {
  color: var(--warning);
}

.catalog-status-empty .catalog-status-title {
  color: var(--text-muted);
}

.catalog-status-error {
  border-color: var(--danger, #c44);
  background: color-mix(in srgb, var(--danger, #c44) 8%, transparent);
}

.catalog-status-error .catalog-status-title {
  color: var(--danger, #c44);
}

.status-msg {
  font-size: 12px;
  color: var(--text-muted);
  min-height: 16px;
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
  color: var(--color-on-accent);
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
  color: var(--color-on-accent);
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
