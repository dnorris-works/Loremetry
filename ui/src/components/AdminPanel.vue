<script setup lang="ts">
import { inject, onMounted, ref } from 'vue';
import { adminFetch, getAdminToken, setAdminToken } from '../api';
import { showPanelKey } from '../injectionKeys';
import type { StaleCleanupResult, WinningCatImportResult } from '../types';

const showPanel = inject(showPanelKey)!;

const adminConfigured = ref(false);
const tokenInput = ref('');
const authenticated = ref(false);
const statusMsg = ref('');

const winningcatStatus = ref('');
const staleStatus = ref('');
const showStaleRow = ref(false);
const importDisabled = ref(false);
let lastImportedAt = '';

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

async function onWinningCatFile(ev: Event): Promise<void> {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;

  winningcatStatus.value = 'Importing...';
  importDisabled.value = true;
  showStaleRow.value = false;
  try {
    const csvText = await file.text();
    const result = await adminFetch<WinningCatImportResult>('/winningcat/import', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ csv_text: csvText }),
    });
    if (result.success) {
      winningcatStatus.value = `✓ Imported ${result.imported} categories. Skipped ${result.skipped_other_department} (other department), ${result.skipped_unparseable} (unparseable).`;
      lastImportedAt = result.imported_at;
      if (result.stale_count > 0) {
        showStaleRow.value = true;
        const word = result.stale_count === 1 ? 'y was' : 'ies were';
        staleStatus.value = `${result.stale_count} categor${word} in the catalog from a previous import but missing from this one — possibly retired or renamed by Amazon.`;
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
  if (!confirm('Remove these stale categories from the catalog? This only affects reference data — no story data is touched.')) return;
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
      Operator tools. Not for end users. WinningCat catalog data is stored in the server database.
    </p>

    <div v-if="!adminConfigured" class="admin-notice">
      Admin API is not configured on this server. Set <code>ADMIN_TOKEN</code> in Miget environment variables, then redeploy.
    </div>

    <template v-else>
      <div v-if="!authenticated" class="settings-form">
        <h3 class="section-title">Sign in</h3>
        <label>Admin token</label>
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
          <span class="token-ok">✓ Admin token set</span>
          <button type="button" class="btn btn-sm" @click="onClearToken">Sign out</button>
        </div>

        <div class="settings-section-divider"></div>
        <h3 class="section-title">WinningCat catalog</h3>
        <div class="settings-form">
          <p class="panel-desc">
            Import the WinningCat browse-node CSV. Books and Kindle Store categories are written to the reference catalog in SQLite.
          </p>
          <label class="btn file-btn" :class="{ disabled: importDisabled }">
            Import CSV
            <input type="file" accept=".csv,text/csv" :disabled="importDisabled" hidden @change="onWinningCatFile" />
          </label>
          <div class="winningcat-status">{{ winningcatStatus }}</div>
          <div v-if="showStaleRow" class="stale-row">
            <div class="stale-status">{{ staleStatus }}</div>
            <button type="button" class="btn btn-sm btn-danger" @click="onRemoveStale">Remove stale</button>
          </div>
        </div>

        <div class="settings-section-divider"></div>
        <h3 class="section-title">CLI upload</h3>
        <pre class="cli-hint">curl -X POST "$APP_URL/api/admin/winningcat/upload" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -F "file=@winningcat.csv"</pre>
      </template>
    </template>
  </div>
</template>

<style scoped>
.admin-panel {
  padding: 20px;
  overflow-y: auto;
  max-width: 640px;
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

.settings-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.settings-form label {
  font-size: 12px;
  color: var(--text-muted);
}

.settings-form input {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  padding: 8px 10px;
  font-size: 13px;
}

.settings-section-divider {
  height: 1px;
  background: var(--border);
  margin: 20px 0;
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

.winningcat-status,
.status-msg,
.stale-status {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.5;
}

.stale-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 4px;
}

.cli-hint {
  font-family: var(--mono);
  font-size: 11px;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--text-muted);
}

.btn-danger {
  background: var(--danger);
  align-self: flex-start;
}
</style>
