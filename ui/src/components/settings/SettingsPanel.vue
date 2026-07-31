<script setup lang="ts">
import { inject, ref, computed } from 'vue';
import { settingsKey, showPanelKey } from '../../injectionKeys';
import { SETTINGS_TABS, type SettingsTab } from './types';
import SettingsGeneralTab from './tabs/SettingsGeneralTab.vue';
import SettingsAiModelsTab from './tabs/SettingsAiModelsTab.vue';
import SettingsCanopyTab from './tabs/SettingsCanopyTab.vue';
import SettingsDataForSeoTab from './tabs/SettingsDataForSeoTab.vue';
import SettingsStoryDataTab from './tabs/SettingsStoryDataTab.vue';
import SettingsArchivedReportsTab from './tabs/SettingsArchivedReportsTab.vue';

const settingsCtx = inject(settingsKey)!;
const showPanel = inject(showPanelKey)!;

const activeTab = ref<SettingsTab>('general');
const savedMsg = ref('');

const showSaveFooter = computed(() =>
  activeTab.value !== 'storydata' && activeTab.value !== 'archived',
);

function onSave(): void {
  settingsCtx.saveSettings().then(() => {
    savedMsg.value = 'Saved';
    setTimeout(() => { savedMsg.value = ''; }, 1500);
  }).catch((e) => {
    savedMsg.value = 'Save failed: ' + String(e);
  });
}
</script>

<template>
  <div class="settings-panel">
    <div class="panel-header">
      <h2 class="panel-title">Settings</h2>
      <button type="button" class="btn btn-sm btn-secondary" @click="showPanel('analyzer')">← Back</button>
    </div>

    <div class="settings-tabs">
      <button
        v-for="tab in SETTINGS_TABS"
        :key="tab.id"
        type="button"
        class="settings-tab"
        :class="{ active: activeTab === tab.id }"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <div class="settings-body">
      <SettingsGeneralTab v-if="activeTab === 'general'" />
      <SettingsAiModelsTab v-else-if="activeTab === 'ai'" />
      <SettingsCanopyTab v-else-if="activeTab === 'canopy'" />
      <SettingsDataForSeoTab v-else-if="activeTab === 'dataforseo'" />
      <SettingsStoryDataTab
        v-else-if="activeTab === 'storydata'"
        :active="activeTab === 'storydata'"
      />
      <SettingsArchivedReportsTab
        v-else-if="activeTab === 'archived'"
        :active="activeTab === 'archived'"
      />
    </div>

    <footer v-if="showSaveFooter" class="settings-footer">
      <button type="button" class="btn" @click="onSave">Save Settings</button>
      <span v-if="savedMsg" class="settings-saved">{{ savedMsg }}</span>
    </footer>
  </div>
</template>

<style scoped>
.settings-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--content-pad, 20px) 24px;
  overflow: hidden;
  box-sizing: border-box;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  flex-shrink: 0;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
}

.settings-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 16px;
  flex-shrink: 0;
}

.settings-tab {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 12px;
  transition: color 0.15s, border-color 0.15s, background 0.15s;
}

.settings-tab:hover {
  color: var(--text);
  border-color: var(--border-strong);
}

.settings-tab.active {
  background: var(--color-accent-subtle);
  border-color: var(--accent);
  color: var(--accent);
}

.settings-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.settings-footer {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 12px;
  padding-top: 16px;
  margin-top: 16px;
  border-top: 1px solid var(--border);
}

.btn-secondary {
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.btn-secondary:hover {
  color: var(--text);
  border-color: var(--accent);
}

.settings-saved {
  font-size: 12px;
  color: var(--success);
}
</style>
