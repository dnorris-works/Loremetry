import { ref } from 'vue';
import { invoke } from '../api';
import type {
  AdCampaign,
  AdCampaignDetail,
  AdCreative,
  AdLandingPage,
  AdPerformanceSnapshot,
  AdSpendEntry,
  AdAudienceNote,
  AdPlatformAccount,
} from '../types';

const campaigns = ref<AdCampaign[]>([]);
const platformAccounts = ref<AdPlatformAccount[]>([]);
const landingPages = ref<AdLandingPage[]>([]);
const campaignDetail = ref<AdCampaignDetail | null>(null);

async function loadCampaigns(storyFolder: string): Promise<void> {
  if (!storyFolder) {
    campaigns.value = [];
    return;
  }
  try {
    const result = await invoke<{ success: boolean; campaigns: AdCampaign[]; error: string }>(
      'list_campaigns',
      { storyFolder },
    );
    campaigns.value = result.success ? result.campaigns : [];
  } catch (e) {
    console.error('loadCampaigns:', e);
    campaigns.value = [];
  }
}

async function createCampaign(storyFolder: string, name: string, platform: string, objective: string): Promise<{ success: boolean; id: number; error: string }> {
  return invoke('create_campaign', {
    request: { story_folder: storyFolder, name, platform, objective },
  });
}

async function updateCampaign(request: {
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
}): Promise<{ success: boolean; error: string }> {
  return invoke('update_campaign', { request });
}

async function deleteCampaign(id: number): Promise<{ success: boolean; error: string }> {
  const result = await invoke<{ success: boolean; error: string }>('delete_campaign', { id });
  if (result.success) {
    campaigns.value = campaigns.value.filter(c => c.id !== id);
  }
  return result;
}

async function loadCampaignDetail(id: number): Promise<AdCampaignDetail | null> {
  try {
    const result = await invoke<{ success: boolean; detail: AdCampaignDetail | null; error: string }>(
      'get_campaign_detail',
      { id },
    );
    campaignDetail.value = result.success ? result.detail : null;
    return campaignDetail.value;
  } catch (e) {
    console.error('loadCampaignDetail:', e);
    campaignDetail.value = null;
    return null;
  }
}

async function createCreative(request: Omit<AdCreative, 'id' | 'created_at' | 'updated_at'>): Promise<{ success: boolean; id: number; error: string }> {
  return invoke('create_creative', { request });
}

async function updateCreative(request: Omit<AdCreative, 'created_at' | 'updated_at'>): Promise<{ success: boolean; error: string }> {
  return invoke('update_creative', { request });
}

async function deleteCreative(id: number): Promise<{ success: boolean; error: string }> {
  return invoke('delete_creative', { id });
}

async function addPerformanceSnapshot(request: Omit<AdPerformanceSnapshot, 'id' | 'created_at'>): Promise<{ success: boolean; id: number; error: string }> {
  return invoke('add_performance_snapshot', { request });
}

async function deletePerformanceSnapshot(id: number): Promise<{ success: boolean; error: string }> {
  return invoke('delete_performance_snapshot', { id });
}

async function addSpendEntry(request: Omit<AdSpendEntry, 'id' | 'created_at'>): Promise<{ success: boolean; id: number; error: string }> {
  return invoke('add_spend_entry', { request });
}

async function deleteSpendEntry(id: number): Promise<{ success: boolean; error: string }> {
  return invoke('delete_spend_entry', { id });
}

async function loadLandingPages(storyFolder: string): Promise<void> {
  if (!storyFolder) {
    landingPages.value = [];
    return;
  }
  try {
    const result = await invoke<{ success: boolean; pages: AdLandingPage[]; error: string }>(
      'list_landing_pages',
      { storyFolder },
    );
    landingPages.value = result.success ? result.pages : [];
  } catch (e) {
    console.error('loadLandingPages:', e);
    landingPages.value = [];
  }
}

async function createLandingPage(request: { story_folder: string; name: string; url: string; conversion_rate: number | null; notes: string }): Promise<{ success: boolean; id: number; error: string }> {
  const result = await invoke<{ success: boolean; id: number; error: string }>('create_landing_page', { request });
  if (result.success) await loadLandingPages(request.story_folder);
  return result;
}

async function updateLandingPage(request: { id: number; name: string; url: string; conversion_rate: number | null; notes: string }, storyFolder: string): Promise<{ success: boolean; error: string }> {
  const result = await invoke<{ success: boolean; error: string }>('update_landing_page', { request });
  if (result.success) await loadLandingPages(storyFolder);
  return result;
}

async function deleteLandingPage(id: number, storyFolder: string): Promise<{ success: boolean; error: string }> {
  const result = await invoke<{ success: boolean; error: string }>('delete_landing_page', { id });
  if (result.success) await loadLandingPages(storyFolder);
  return result;
}

async function addAudienceNote(request: Omit<AdAudienceNote, 'id' | 'created_at'>): Promise<{ success: boolean; id: number; error: string }> {
  return invoke('add_audience_note', { request });
}

async function updateAudienceNote(request: Omit<AdAudienceNote, 'created_at'>): Promise<{ success: boolean; error: string }> {
  return invoke('update_audience_note', { request });
}

async function deleteAudienceNote(id: number): Promise<{ success: boolean; error: string }> {
  return invoke('delete_audience_note', { id });
}

async function loadPlatformAccounts(): Promise<void> {
  try {
    const result = await invoke<{ success: boolean; accounts: AdPlatformAccount[]; error: string }>('list_platform_accounts');
    platformAccounts.value = result.success ? result.accounts : [];
  } catch (e) {
    console.error('loadPlatformAccounts:', e);
    platformAccounts.value = [];
  }
}

async function createPlatformAccount(request: { platform: string; account_id: string; pixel_id: string; tracking_notes: string; payment_notes: string }): Promise<{ success: boolean; id: number; error: string }> {
  const result = await invoke<{ success: boolean; id: number; error: string }>('create_platform_account', { request });
  if (result.success) await loadPlatformAccounts();
  return result;
}

async function updatePlatformAccount(request: { id: number; platform: string; account_id: string; pixel_id: string; tracking_notes: string; payment_notes: string }): Promise<{ success: boolean; error: string }> {
  const result = await invoke<{ success: boolean; error: string }>('update_platform_account', { request });
  if (result.success) await loadPlatformAccounts();
  return result;
}

async function deletePlatformAccount(id: number): Promise<{ success: boolean; error: string }> {
  const result = await invoke<{ success: boolean; error: string }>('delete_platform_account', { id });
  if (result.success) await loadPlatformAccounts();
  return result;
}

export function useCampaigns() {
  return {
    campaigns,
    platformAccounts,
    landingPages,
    campaignDetail,
    loadCampaigns,
    createCampaign,
    updateCampaign,
    deleteCampaign,
    loadCampaignDetail,
    createCreative,
    updateCreative,
    deleteCreative,
    addPerformanceSnapshot,
    deletePerformanceSnapshot,
    addSpendEntry,
    deleteSpendEntry,
    loadLandingPages,
    createLandingPage,
    updateLandingPage,
    deleteLandingPage,
    addAudienceNote,
    updateAudienceNote,
    deleteAudienceNote,
    loadPlatformAccounts,
    createPlatformAccount,
    updatePlatformAccount,
    deletePlatformAccount,
  };
}
