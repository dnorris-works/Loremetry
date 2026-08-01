<script setup lang="ts">
import { inject, ref, onMounted } from 'vue';
import { campaignsKey } from '../../injectionKeys';
import type { AdPlatformAccount } from '../../types';

const emit = defineEmits<{
  (e: 'back'): void;
}>();

const campaignsCtx = inject(campaignsKey)!;

const showForm = ref(false);
const editingId = ref<number | null>(null);
const platform = ref('meta');
const accountId = ref('');
const pixelId = ref('');
const trackingNotes = ref('');
const paymentNotes = ref('');
const error = ref('');

const PLATFORMS = ['meta', 'amazon', 'tiktok', 'google', 'bookbub', 'other'];

onMounted(() => {
  void campaignsCtx.loadPlatformAccounts();
});

function resetForm(): void {
  showForm.value = false;
  editingId.value = null;
  platform.value = 'meta';
  accountId.value = '';
  pixelId.value = '';
  trackingNotes.value = '';
  paymentNotes.value = '';
  error.value = '';
}

function editAccount(a: AdPlatformAccount): void {
  editingId.value = a.id;
  platform.value = a.platform;
  accountId.value = a.account_id;
  pixelId.value = a.pixel_id;
  trackingNotes.value = a.tracking_notes;
  paymentNotes.value = a.payment_notes;
  showForm.value = true;
}

async function onSave(): Promise<void> {
  error.value = '';
  if (editingId.value) {
    const result = await campaignsCtx.updatePlatformAccount({
      id: editingId.value,
      platform: platform.value,
      account_id: accountId.value,
      pixel_id: pixelId.value,
      tracking_notes: trackingNotes.value,
      payment_notes: paymentNotes.value,
    });
    if (!result.success) { error.value = result.error; return; }
  } else {
    const result = await campaignsCtx.createPlatformAccount({
      platform: platform.value,
      account_id: accountId.value,
      pixel_id: pixelId.value,
      tracking_notes: trackingNotes.value,
      payment_notes: paymentNotes.value,
    });
    if (!result.success) { error.value = result.error; return; }
  }
  resetForm();
}

async function onDelete(id: number): Promise<void> {
  if (!confirm('Delete this platform account reference?')) return;
  await campaignsCtx.deletePlatformAccount(id);
}
</script>

<template>
  <div class="marketing-panel">
    <div class="panel-header-row">
      <h2 class="panel-title">Platform accounts</h2>
      <div class="header-actions">
        <button type="button" class="btn btn-sm" @click="emit('back')">Campaigns</button>
        <button type="button" class="btn btn-sm" @click="showForm = true; editingId = null">Add account</button>
      </div>
    </div>

    <div class="panel-body-scroll">
      <p class="panel-desc">
        Reference data for ad accounts — IDs, pixels, and payment notes. API tokens belong in Settings.
      </p>

      <div v-if="showForm" class="form-card">
        <h3 class="form-card-title">{{ editingId ? 'Edit account' : 'Add account' }}</h3>
        <div class="form-group">
          <label>Platform</label>
          <select v-model="platform">
            <option v-for="p in PLATFORMS" :key="p" :value="p">{{ p }}</option>
          </select>
        </div>
        <div class="form-group">
          <label>Account ID</label>
          <input v-model="accountId" type="text" placeholder="Ad account ID" />
        </div>
        <div class="form-group">
          <label>Pixel / tag ID</label>
          <input v-model="pixelId" type="text" />
        </div>
        <div class="form-group">
          <label>Tracking notes</label>
          <textarea v-model="trackingNotes" rows="2" />
        </div>
        <div class="form-group">
          <label>Payment notes</label>
          <textarea v-model="paymentNotes" rows="2" />
        </div>
        <div v-if="error" class="form-error">{{ error }}</div>
        <div class="form-actions">
          <button type="button" class="btn btn-sm" @click="onSave">Save</button>
          <button type="button" class="btn btn-sm btn-secondary" @click="resetForm">Cancel</button>
        </div>
      </div>

      <p v-if="campaignsCtx.platformAccounts.value.length === 0 && !showForm" class="panel-desc">
        No platform accounts yet.
      </p>

      <div class="account-list">
        <div v-for="a in campaignsCtx.platformAccounts.value" :key="a.id" class="form-card">
          <div class="account-top">
            <span class="tag">{{ a.platform }}</span>
            <span class="account-id">{{ a.account_id || 'no ID' }}</span>
          </div>
          <p v-if="a.pixel_id" class="account-detail">Pixel: {{ a.pixel_id }}</p>
          <p v-if="a.tracking_notes" class="account-detail">{{ a.tracking_notes }}</p>
          <p v-if="a.payment_notes" class="account-detail">Payment: {{ a.payment_notes }}</p>
          <div class="form-actions">
            <button type="button" class="btn btn-sm" @click="editAccount(a)">Edit</button>
            <button type="button" class="btn btn-sm btn-danger" @click="onDelete(a.id)">Delete</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.marketing-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.panel-header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 20px 24px 12px;
  border-bottom: 1px solid var(--border);
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin: 0;
}

.header-actions {
  display: flex;
  gap: 8px;
}

.panel-body-scroll {
  flex: 1;
  overflow: auto;
  padding: 16px 24px 24px;
  max-width: 560px;
}

.form-card {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px;
  margin-bottom: 12px;
  background: var(--surface);
}

.form-card-title {
  font-size: 14px;
  margin: 0 0 12px;
}

.form-group {
  margin-bottom: 12px;
}

.form-group label {
  display: block;
  font-size: 11px;
  color: var(--text-muted);
  margin-bottom: 4px;
  text-transform: uppercase;
}

.form-group input,
.form-group select,
.form-group textarea {
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface2);
  color: var(--text);
  font-size: 13px;
}

.form-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.form-error {
  color: var(--danger, #c44);
  font-size: 13px;
}

.account-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.account-top {
  display: flex;
  align-items: center;
  gap: 10px;
}

.tag {
  font-size: 11px;
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: 999px;
  text-transform: capitalize;
}

.account-id {
  font-size: 12px;
  color: var(--text-muted);
}

.account-detail {
  font-size: 12px;
  color: var(--text-muted);
  margin: 6px 0 0;
}
</style>
