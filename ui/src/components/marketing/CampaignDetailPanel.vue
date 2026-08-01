<script setup lang="ts">
import { inject, ref, watch, computed } from 'vue';
import { storiesKey, campaignsKey } from '../../injectionKeys';

const props = defineProps<{
  campaignId: number;
}>();

const emit = defineEmits<{
  (e: 'back'): void;
  (e: 'edit'): void;
}>();

const storiesCtx = inject(storiesKey)!;
const campaignsCtx = inject(campaignsKey)!;

type DetailTab = 'overview' | 'creatives' | 'metrics' | 'spend' | 'landing' | 'audience';
const activeTab = ref<DetailTab>('overview');
const loading = ref(false);
const error = ref('');

const detail = computed(() => campaignsCtx.campaignDetail.value);
const campaign = computed(() => detail.value?.campaign);
const storyFolder = computed(() => storiesCtx.activeFolder.value);

const tabs: { id: DetailTab; label: string }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'creatives', label: 'Creatives' },
  { id: 'metrics', label: 'Metrics' },
  { id: 'spend', label: 'Spend' },
  { id: 'landing', label: 'Landing' },
  { id: 'audience', label: 'Audience' },
];

const showCreativeForm = ref(false);
const editingCreativeId = ref<number | null>(null);
const crName = ref('');
const crType = ref('video');
const crVersion = ref('v1');
const crFormat = ref('');
const crStatus = ref('draft');
const crAssetPath = ref('');
const crBodyText = ref('');

const showMetricsForm = ref(false);
const mDate = ref('');
const mImpressions = ref(0);
const mClicks = ref(0);
const mConversions = ref(0);
const mCtr = ref(0);
const mCpc = ref(0);
const mCpa = ref(0);
const mSpend = ref(0);

const showSpendForm = ref(false);
const sPlatform = ref('');
const sAmount = ref(0);
const sDate = ref('');
const sNotes = ref('');

const showLandingForm = ref(false);
const lpName = ref('');
const lpUrl = ref('');
const lpConversion = ref<number | null>(null);
const lpNotes = ref('');

const showAudienceForm = ref(false);
const auLabel = ref('');
const auDemographics = ref('');
const auInterests = ref('');
const auLookalike = ref('');
const auOutcome = ref('untested');
const auNotes = ref('');

async function reload(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    await campaignsCtx.loadCampaignDetail(props.campaignId);
    if (storyFolder.value) await campaignsCtx.loadLandingPages(storyFolder.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

watch(() => props.campaignId, () => { void reload(); }, { immediate: true });

function resetCreativeForm(): void {
  showCreativeForm.value = false;
  editingCreativeId.value = null;
  crName.value = '';
  crType.value = 'video';
  crVersion.value = 'v1';
  crFormat.value = '';
  crStatus.value = 'draft';
  crAssetPath.value = '';
  crBodyText.value = '';
}

async function saveCreative(): Promise<void> {
  if (!crName.value.trim()) return;
  if (editingCreativeId.value) {
    await campaignsCtx.updateCreative({
      id: editingCreativeId.value,
      campaign_id: props.campaignId,
      name: crName.value.trim(),
      creative_type: crType.value,
      version: crVersion.value,
      platform_format: crFormat.value,
      status: crStatus.value,
      asset_path: crAssetPath.value,
      body_text: crBodyText.value,
      notes: '',
    });
  } else {
    await campaignsCtx.createCreative({
      campaign_id: props.campaignId,
      name: crName.value.trim(),
      creative_type: crType.value,
      version: crVersion.value,
      platform_format: crFormat.value,
      status: crStatus.value,
      asset_path: crAssetPath.value,
      body_text: crBodyText.value,
      notes: '',
    });
  }
  resetCreativeForm();
  await reload();
}

async function saveMetrics(): Promise<void> {
  if (!mDate.value) return;
  await campaignsCtx.addPerformanceSnapshot({
    campaign_id: props.campaignId,
    creative_id: null,
    snapshot_date: mDate.value,
    impressions: mImpressions.value,
    clicks: mClicks.value,
    conversions: mConversions.value,
    ctr: mCtr.value,
    cpc: mCpc.value,
    cpa: mCpa.value,
    spend: mSpend.value,
    notes: '',
  });
  showMetricsForm.value = false;
  await reload();
}

async function saveSpend(): Promise<void> {
  if (!sDate.value || sAmount.value <= 0) return;
  await campaignsCtx.addSpendEntry({
    campaign_id: props.campaignId,
    platform: sPlatform.value || campaign.value?.platform || '',
    amount: sAmount.value,
    spent_at: sDate.value,
    notes: sNotes.value,
  });
  showSpendForm.value = false;
  await reload();
}

async function saveLanding(): Promise<void> {
  if (!storyFolder.value || !lpName.value.trim()) return;
  const result = await campaignsCtx.createLandingPage({
    story_folder: storyFolder.value,
    name: lpName.value.trim(),
    url: lpUrl.value,
    conversion_rate: lpConversion.value,
    notes: lpNotes.value,
  });
  if (result.success && campaign.value) {
    await campaignsCtx.updateCampaign({
      id: props.campaignId,
      name: campaign.value.name,
      platform: campaign.value.platform,
      platform_account_id: campaign.value.platform_account_id,
      objective: campaign.value.objective,
      status: campaign.value.status,
      budget: campaign.value.budget,
      budget_period: campaign.value.budget_period,
      start_date: campaign.value.start_date,
      end_date: campaign.value.end_date,
      target_audience: campaign.value.target_audience,
      landing_page_id: result.id,
      notes: campaign.value.notes,
    });
    showLandingForm.value = false;
    await reload();
  }
}

async function saveAudience(): Promise<void> {
  await campaignsCtx.addAudienceNote({
    campaign_id: props.campaignId,
    label: auLabel.value,
    demographics: auDemographics.value,
    interests: auInterests.value,
    lookalike_notes: auLookalike.value,
    outcome: auOutcome.value,
    notes: auNotes.value,
  });
  showAudienceForm.value = false;
  auLabel.value = '';
  auDemographics.value = '';
  auInterests.value = '';
  auLookalike.value = '';
  auOutcome.value = 'untested';
  auNotes.value = '';
  await reload();
}

async function onDeleteCampaign(): Promise<void> {
  if (!confirm('Delete this campaign and all its data?')) return;
  await campaignsCtx.deleteCampaign(props.campaignId);
  if (storyFolder.value) await campaignsCtx.loadCampaigns(storyFolder.value);
  emit('back');
}
</script>

<template>
  <div class="detail-root">
    <div class="panel-header-row">
      <h2 class="panel-title">{{ campaign?.name || 'Campaign' }}</h2>
      <div class="header-actions">
        <button type="button" class="btn btn-sm" @click="emit('back')">Back</button>
        <button type="button" class="btn btn-sm" @click="emit('edit')">Edit</button>
        <button type="button" class="btn btn-sm btn-danger" @click="onDeleteCampaign">Delete</button>
      </div>
    </div>

    <div v-if="campaign" class="tab-row">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.id }"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <div class="detail-body">
      <p v-if="loading">Loading…</p>
      <p v-else-if="error" class="form-error">{{ error }}</p>

      <template v-else-if="campaign">
        <div v-if="activeTab === 'overview'" class="overview-grid">
          <div><span class="label">Platform</span>{{ campaign.platform }}</div>
          <div><span class="label">Objective</span>{{ campaign.objective }}</div>
          <div><span class="label">Status</span>{{ campaign.status }}</div>
          <div>
            <span class="label">Budget</span>
            {{ campaign.budget != null ? `$${campaign.budget} (${campaign.budget_period})` : '—' }}
          </div>
          <div>
            <span class="label">Dates</span>
            {{ campaign.start_date || '—' }} – {{ campaign.end_date || '—' }}
          </div>
          <div>
            <span class="label">Total spend</span>
            ${{ (campaign.total_spend || 0).toFixed(2) }}
          </div>
          <div v-if="campaign.target_audience" class="full-width">
            <span class="label">Audience</span>{{ campaign.target_audience }}
          </div>
          <div v-if="detail?.landing_page" class="full-width">
            <span class="label">Landing</span>
            <a :href="detail.landing_page.url" target="_blank" rel="noopener">{{ detail.landing_page.name }}</a>
          </div>
          <div v-if="campaign.notes" class="full-width">
            <span class="label">Notes</span>{{ campaign.notes }}
          </div>
        </div>

        <div v-else-if="activeTab === 'creatives'">
          <button type="button" class="btn btn-sm" @click="showCreativeForm = true; editingCreativeId = null">Add creative</button>
          <div v-if="showCreativeForm" class="form-card">
            <input v-model="crName" placeholder="Name" />
            <select v-model="crType">
              <option value="video">Video</option>
              <option value="thumbnail">Thumbnail</option>
              <option value="hook">Hook</option>
              <option value="caption">Caption</option>
              <option value="cta">CTA</option>
            </select>
            <input v-model="crVersion" placeholder="Version" />
            <input v-model="crFormat" placeholder="Platform format" />
            <select v-model="crStatus">
              <option value="draft">Draft</option>
              <option value="live">Live</option>
              <option value="paused">Paused</option>
              <option value="retired">Retired</option>
            </select>
            <input v-model="crAssetPath" placeholder="Asset path / URL" />
            <textarea v-model="crBodyText" rows="2" placeholder="Copy" />
            <div class="form-actions">
              <button type="button" class="btn btn-sm" @click="saveCreative">Save</button>
              <button type="button" class="btn btn-sm btn-secondary" @click="resetCreativeForm">Cancel</button>
            </div>
          </div>
          <p v-if="!detail?.creatives.length" class="panel-desc">No creatives yet.</p>
          <div v-for="c in detail?.creatives" :key="c.id" class="form-card">
            <strong>{{ c.name }}</strong> · {{ c.creative_type }} · {{ c.status }}
            <p v-if="c.body_text" class="muted">{{ c.body_text }}</p>
            <button type="button" class="btn btn-sm btn-danger" @click="campaignsCtx.deleteCreative(c.id).then(reload)">Delete</button>
          </div>
        </div>

        <div v-else-if="activeTab === 'metrics'">
          <button type="button" class="btn btn-sm" @click="showMetricsForm = true">Log snapshot</button>
          <div v-if="showMetricsForm" class="form-card">
            <input v-model="mDate" placeholder="YYYY-MM-DD" />
            <div class="form-row">
              <input v-model.number="mImpressions" type="number" placeholder="Impressions" />
              <input v-model.number="mClicks" type="number" placeholder="Clicks" />
              <input v-model.number="mConversions" type="number" placeholder="Conversions" />
            </div>
            <div class="form-row">
              <input v-model.number="mCtr" type="number" step="0.01" placeholder="CTR %" />
              <input v-model.number="mCpc" type="number" step="0.01" placeholder="CPC" />
              <input v-model.number="mCpa" type="number" step="0.01" placeholder="CPA" />
              <input v-model.number="mSpend" type="number" step="0.01" placeholder="Spend" />
            </div>
            <button type="button" class="btn btn-sm" @click="saveMetrics">Save</button>
          </div>
          <table v-if="detail?.snapshots.length" class="data-table">
            <thead>
              <tr>
                <th>Date</th><th>Impr.</th><th>Clicks</th><th>CTR</th><th>Spend</th><th></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="s in detail.snapshots" :key="s.id">
                <td>{{ s.snapshot_date }}</td>
                <td>{{ s.impressions }}</td>
                <td>{{ s.clicks }}</td>
                <td>{{ s.ctr }}%</td>
                <td>${{ s.spend }}</td>
                <td>
                  <button type="button" class="btn btn-sm btn-danger" @click="campaignsCtx.deletePerformanceSnapshot(s.id).then(reload)">×</button>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else class="panel-desc">No metrics logged yet.</p>
        </div>

        <div v-else-if="activeTab === 'spend'">
          <button type="button" class="btn btn-sm" @click="showSpendForm = true">Log spend</button>
          <div v-if="showSpendForm" class="form-card">
            <input v-model="sPlatform" :placeholder="campaign.platform" />
            <input v-model.number="sAmount" type="number" step="0.01" placeholder="Amount" />
            <input v-model="sDate" placeholder="YYYY-MM-DD" />
            <input v-model="sNotes" placeholder="Notes" />
            <button type="button" class="btn btn-sm" @click="saveSpend">Save</button>
          </div>
          <table v-if="detail?.spend_entries.length" class="data-table">
            <thead><tr><th>Date</th><th>Platform</th><th>Amount</th><th></th></tr></thead>
            <tbody>
              <tr v-for="e in detail.spend_entries" :key="e.id">
                <td>{{ e.spent_at }}</td>
                <td>{{ e.platform }}</td>
                <td>${{ e.amount.toFixed(2) }}</td>
                <td>
                  <button type="button" class="btn btn-sm btn-danger" @click="campaignsCtx.deleteSpendEntry(e.id).then(reload)">×</button>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else class="panel-desc">No spend logged yet.</p>
        </div>

        <div v-else-if="activeTab === 'landing'">
          <div v-if="detail?.landing_page" class="form-card">
            <strong>{{ detail.landing_page.name }}</strong>
            <p><a :href="detail.landing_page.url" target="_blank" rel="noopener">{{ detail.landing_page.url }}</a></p>
          </div>
          <template v-else>
            <p class="panel-desc">No landing page linked.</p>
            <button type="button" class="btn btn-sm" @click="showLandingForm = true">Create & link</button>
          </template>
          <div v-if="showLandingForm" class="form-card">
            <input v-model="lpName" placeholder="Page name" />
            <input v-model="lpUrl" placeholder="https://…" />
            <input v-model.number="lpConversion" type="number" step="0.1" placeholder="Conversion %" />
            <textarea v-model="lpNotes" rows="2" placeholder="Notes" />
            <button type="button" class="btn btn-sm" @click="saveLanding">Save & link</button>
          </div>
        </div>

        <div v-else-if="activeTab === 'audience'">
          <button type="button" class="btn btn-sm" @click="showAudienceForm = true">Add note</button>
          <div v-if="showAudienceForm" class="form-card">
            <input v-model="auLabel" placeholder="Label" />
            <textarea v-model="auDemographics" rows="2" placeholder="Demographics" />
            <textarea v-model="auInterests" rows="2" placeholder="Interests" />
            <textarea v-model="auLookalike" rows="2" placeholder="Lookalike notes" />
            <select v-model="auOutcome">
              <option value="untested">Untested</option>
              <option value="worked">Worked</option>
              <option value="poor">Poor</option>
            </select>
            <textarea v-model="auNotes" rows="2" placeholder="Notes" />
            <button type="button" class="btn btn-sm" @click="saveAudience">Save</button>
          </div>
          <div v-for="n in detail?.audience_notes" :key="n.id" class="form-card">
            <strong>{{ n.label || 'Untitled' }}</strong> · {{ n.outcome }}
            <p v-if="n.demographics" class="muted">{{ n.demographics }}</p>
            <button type="button" class="btn btn-sm btn-danger" @click="campaignsCtx.deleteAudienceNote(n.id).then(reload)">Delete</button>
          </div>
          <p v-if="!detail?.audience_notes.length" class="panel-desc">No audience tests recorded.</p>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.detail-root {
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

.tab-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 12px 24px;
  border-bottom: 1px solid var(--border);
}

.tab-btn {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 12px;
  padding: 6px 12px;
  cursor: pointer;
}

.tab-btn.active {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--color-on-accent);
}

.detail-body {
  flex: 1;
  overflow: auto;
  padding: 16px 24px 24px;
  max-width: 720px;
}

.overview-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  font-size: 13px;
}

.overview-grid .full-width {
  grid-column: 1 / -1;
}

.label {
  display: block;
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  margin-bottom: 2px;
}

.form-card {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
  margin: 12px 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: var(--surface);
}

.form-card input,
.form-card select,
.form-card textarea {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface2);
  color: var(--text);
  font-size: 13px;
}

.form-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
  gap: 8px;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  margin-top: 12px;
}

.data-table th,
.data-table td {
  border-bottom: 1px solid var(--border);
  padding: 6px 8px;
  text-align: left;
}

.muted {
  font-size: 12px;
  color: var(--text-muted);
  margin: 4px 0 0;
}

.form-error {
  color: var(--danger, #c44);
}

.form-actions {
  display: flex;
  gap: 8px;
}
</style>
