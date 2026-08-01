<script setup lang="ts">
import { inject, ref, watch, computed } from 'vue';
import { storiesKey, campaignsKey } from '../../injectionKeys';

const props = defineProps<{
  campaignId: number | null;
}>();

const emit = defineEmits<{
  (e: 'saved', id: number): void;
  (e: 'cancel'): void;
}>();

const storiesCtx = inject(storiesKey)!;
const campaignsCtx = inject(campaignsKey)!;

const name = ref('');
const platform = ref('meta');
const objective = ref('awareness');
const status = ref('draft');
const budget = ref<number | null>(null);
const budgetPeriod = ref('lifetime');
const startDate = ref('');
const endDate = ref('');
const targetAudience = ref('');
const landingPageId = ref<number | null>(null);
const platformAccountId = ref<number | null>(null);
const notes = ref('');
const error = ref('');
const saving = ref(false);

const isEditing = computed(() => props.campaignId != null && props.campaignId > 0);
const storyFolder = computed(() => storiesCtx.activeFolder.value);
const panelTitle = computed(() => (isEditing.value ? 'Edit campaign' : 'New campaign'));

const PLATFORMS = ['meta', 'amazon', 'tiktok', 'google', 'bookbub', 'other'];
const OBJECTIVES = ['awareness', 'conversion', 'traffic', 'engagement'];
const STATUSES = ['draft', 'active', 'paused', 'archived'];

watch(() => props.campaignId, async (id) => {
  error.value = '';
  if (!id) {
    name.value = '';
    platform.value = 'meta';
    objective.value = 'awareness';
    status.value = 'draft';
    budget.value = null;
    budgetPeriod.value = 'lifetime';
    startDate.value = '';
    endDate.value = '';
    targetAudience.value = '';
    landingPageId.value = null;
    platformAccountId.value = null;
    notes.value = '';
    return;
  }
  const detail = await campaignsCtx.loadCampaignDetail(id);
  if (!detail) return;
  const c = detail.campaign;
  name.value = c.name;
  platform.value = c.platform || 'meta';
  objective.value = c.objective || 'awareness';
  status.value = c.status || 'draft';
  budget.value = c.budget;
  budgetPeriod.value = c.budget_period || 'lifetime';
  startDate.value = c.start_date || '';
  endDate.value = c.end_date || '';
  targetAudience.value = c.target_audience || '';
  landingPageId.value = c.landing_page_id;
  platformAccountId.value = c.platform_account_id;
  notes.value = c.notes || '';
}, { immediate: true });

watch(storyFolder, (folder) => {
  if (folder) void campaignsCtx.loadLandingPages(folder);
  void campaignsCtx.loadPlatformAccounts();
}, { immediate: true });

async function onSave(): Promise<void> {
  const trimName = name.value.trim();
  if (!trimName) { error.value = 'Campaign name is required.'; return; }
  if (!storyFolder.value) { error.value = 'Select a story first.'; return; }
  saving.value = true;
  error.value = '';
  try {
    if (isEditing.value && props.campaignId) {
      const result = await campaignsCtx.updateCampaign({
        id: props.campaignId,
        name: trimName,
        platform: platform.value,
        platform_account_id: platformAccountId.value,
        objective: objective.value,
        status: status.value,
        budget: budget.value,
        budget_period: budgetPeriod.value,
        start_date: startDate.value,
        end_date: endDate.value,
        target_audience: targetAudience.value,
        landing_page_id: landingPageId.value,
        notes: notes.value,
      });
      if (!result.success) { error.value = result.error; return; }
      emit('saved', props.campaignId);
    } else {
      const result = await campaignsCtx.createCampaign(storyFolder.value, trimName, platform.value, objective.value);
      if (!result.success) { error.value = result.error; return; }
      await campaignsCtx.updateCampaign({
        id: result.id,
        name: trimName,
        platform: platform.value,
        platform_account_id: platformAccountId.value,
        objective: objective.value,
        status: status.value,
        budget: budget.value,
        budget_period: budgetPeriod.value,
        start_date: startDate.value,
        end_date: endDate.value,
        target_audience: targetAudience.value,
        landing_page_id: landingPageId.value,
        notes: notes.value,
      });
      await campaignsCtx.loadCampaigns(storyFolder.value);
      emit('saved', result.id);
    }
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="marketing-form panel">
    <h2 class="panel-title">{{ panelTitle }}</h2>

    <div class="form-group">
      <label>Name</label>
      <input v-model="name" type="text" placeholder="e.g. Book 1 Launch — Meta" />
    </div>

    <div class="form-row">
      <div class="form-group">
        <label>Platform</label>
        <select v-model="platform">
          <option v-for="p in PLATFORMS" :key="p" :value="p">{{ p }}</option>
        </select>
      </div>
      <div class="form-group">
        <label>Objective</label>
        <select v-model="objective">
          <option v-for="o in OBJECTIVES" :key="o" :value="o">{{ o }}</option>
        </select>
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label>Status</label>
        <select v-model="status">
          <option v-for="s in STATUSES" :key="s" :value="s">{{ s }}</option>
        </select>
      </div>
      <div class="form-group">
        <label>Platform account</label>
        <select v-model="platformAccountId">
          <option :value="null">—</option>
          <option
            v-for="a in campaignsCtx.platformAccounts.value"
            :key="a.id"
            :value="a.id"
          >
            {{ a.platform }} — {{ a.account_id || 'no ID' }}
          </option>
        </select>
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label>Budget</label>
        <input v-model.number="budget" type="number" min="0" step="0.01" placeholder="0.00" />
      </div>
      <div class="form-group">
        <label>Budget period</label>
        <select v-model="budgetPeriod">
          <option value="daily">Daily</option>
          <option value="lifetime">Lifetime</option>
        </select>
      </div>
    </div>

    <div class="form-row">
      <div class="form-group">
        <label>Start date</label>
        <input v-model="startDate" type="text" placeholder="YYYY-MM-DD" />
      </div>
      <div class="form-group">
        <label>End date</label>
        <input v-model="endDate" type="text" placeholder="YYYY-MM-DD" />
      </div>
    </div>

    <div class="form-group">
      <label>Landing page</label>
      <select v-model="landingPageId">
        <option :value="null">—</option>
        <option v-for="lp in campaignsCtx.landingPages.value" :key="lp.id" :value="lp.id">
          {{ lp.name }} — {{ lp.url }}
        </option>
      </select>
    </div>

    <div class="form-group">
      <label>Target audience</label>
      <textarea v-model="targetAudience" rows="3" placeholder="Demographics, interests, lookalikes…" />
    </div>

    <div class="form-group">
      <label>Notes</label>
      <textarea v-model="notes" rows="2" />
    </div>

    <div v-if="error" class="form-error">{{ error }}</div>

    <div class="form-actions">
      <button type="button" class="btn" :disabled="saving" @click="onSave">Save</button>
      <button type="button" class="btn btn-secondary" @click="emit('cancel')">Cancel</button>
    </div>
  </div>
</template>

<style scoped>
.marketing-form {
  padding: 20px 24px;
  max-width: 520px;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin: 0 0 16px;
}

.form-group {
  margin-bottom: 14px;
}

.form-group label {
  display: block;
  font-size: 12px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 6px;
}

.form-group input,
.form-group select,
.form-group textarea {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  font-size: 13px;
  padding: 8px 10px;
  width: 100%;
}

.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.form-error {
  color: var(--danger, #c44);
  font-size: 13px;
  margin-bottom: 12px;
}

.form-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}
</style>
