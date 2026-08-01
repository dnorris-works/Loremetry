<script setup lang="ts">
import { ref, watch, onMounted, provide, computed } from 'vue';
import { useStories } from '../composables/useStories';
import { useAnalysis } from '../composables/useAnalysis';
import { usePlatform } from '../composables/usePlatform';
import { useSettings } from '../composables/useSettings';
import { useReports } from '../composables/useReports';
import { useSeries } from '../composables/useSeries';
import { useCampaigns } from '../composables/useCampaigns';
import { useResizableWidth } from '../composables/useResizableWidth';
import {
  storiesKey, analysisKey, platformKey, settingsKey,
  reportsKey, seriesKey, campaignsKey, showPanelKey, openManuscriptEditorKey,
} from '../injectionKeys';
import type { Story, Finding, Series } from '../types';

import TitleBar from './TitleBar.vue';
import Sidebar from './Sidebar.vue';
import AnalyzerPanel from './AnalyzerPanel.vue';
import SavedReportsPanel from './SavedReportsPanel.vue';
import ReportsViewer from './ReportsViewer.vue';
import AdminPanel from './AdminPanel.vue';
import SettingsPanel from './settings/SettingsPanel.vue';
import StoryForm from './StoryForm.vue';
import StorySourcesPanel from './StorySourcesPanel.vue';
import SeriesForm from './SeriesForm.vue';
import NewDocumentForm from './NewDocumentForm.vue';
import ManuscriptViewer from './ManuscriptViewer.vue';
import WritingPanel from './WritingPanel.vue';
import CampaignsPanel from './marketing/CampaignsPanel.vue';
import CampaignForm from './marketing/CampaignForm.vue';
import CampaignDetailPanel from './marketing/CampaignDetailPanel.vue';
import PlatformAccountsPanel from './marketing/PlatformAccountsPanel.vue';
import HelpPanel from './HelpPanel.vue';
import StatusFooter from './StatusFooter.vue';
import { useAuth } from '../composables/useAuth';
import { useReportTypes } from '../composables/useReportTypes';
import { useServiceHealth } from '../composables/useServiceHealth';
import { maintenanceMode } from '../api';
import MaintenanceScreen from './MaintenanceScreen.vue';

const auth = useAuth();
provide('isAdmin', auth.isAdmin);

const { loadReportTypes } = useReportTypes();
const { checkHealth } = useServiceHealth();

const storiesCtx = useStories();
const analysisCtx = useAnalysis();
const platformCtx = usePlatform();
const settingsCtx = useSettings();
const reportsCtx = useReports();
const seriesCtx = useSeries();
const campaignsCtx = useCampaigns();

provide(storiesKey, storiesCtx);
provide(analysisKey, analysisCtx);
provide(platformKey, platformCtx);
provide(settingsKey, settingsCtx);
provide(reportsKey, reportsCtx);
provide(seriesKey, seriesCtx);
provide(campaignsKey, campaignsCtx);

type AppMode = 'analyzer' | 'writing' | 'marketing';
const appMode = ref<AppMode>('analyzer');

provide('appMode', appMode);
provide('setAppMode', (mode: AppMode) => {
  appMode.value = mode;
  if (mode === 'marketing') {
    activePanel.value = 'campaigns';
  } else if (['campaigns', 'campaign-detail', 'campaign-form', 'platform-accounts'].includes(activePanel.value)) {
    activePanel.value = 'analyzer';
  }
});

type Panel = 'analyzer' | 'reports' | 'admin' | 'settings' | 'help' | 'story-form' | 'series' | 'manuscript' | 'new-document' | 'sources' | 'campaigns' | 'campaign-detail' | 'campaign-form' | 'platform-accounts';
const activePanel = ref<Panel>('analyzer');
const prevPanel = ref<Panel>('analyzer');
const sidebarOpen = ref(false);
const sourcesWizard = ref(false);
const panelBeforeNewDoc = ref<Panel>('analyzer');
const modeBeforeNewDoc = ref<AppMode>('analyzer');

const { width: sidebarWidth, startResize: startSidebarResize } = useResizableWidth({
  storageKey: 'sidebar-width',
  defaultWidth: 220,
  min: 160,
  max: 480,
});

const appRootStyle = computed(() => ({
  '--sidebar-w': `${sidebarWidth.value}px`,
}));

function toggleSidebar(): void {
  sidebarOpen.value = !sidebarOpen.value;
}

function closeSidebar(): void {
  sidebarOpen.value = false;
}

provide('toggleSidebar', toggleSidebar);
provide('closeSidebar', closeSidebar);

function showPanel(name: Panel): void {
  if ((name === 'settings' || name === 'help') && activePanel.value === name) {
    activePanel.value = prevPanel.value;
    sidebarOpen.value = false;
    return;
  }
  if ((name === 'settings' || name === 'help') && activePanel.value !== name) {
    prevPanel.value = activePanel.value;
  }
  activePanel.value = name;
  sidebarOpen.value = false;
}

function openSources(wizard = false): void {
  sourcesWizard.value = wizard;
  appMode.value = 'analyzer';
  showPanel('sources');
}

provide(showPanelKey, showPanel as (name: string) => void);
provide('openSources', openSources);

const fileTreeTick = ref(0);
provide('fileTreeTick', fileTreeTick);

const manuscriptFindings = ref<Finding[]>([]);
const manuscriptStartIndex = ref(0);
const manuscriptReturnPanel = ref<Panel>('analyzer');

function openManuscriptEditor(findings: Finding[], startIndex: number): void {
  manuscriptFindings.value = findings;
  manuscriptStartIndex.value = startIndex;
  if (activePanel.value === 'reports') {
    manuscriptReturnPanel.value = 'reports';
  } else if (activePanel.value !== 'manuscript') {
    manuscriptReturnPanel.value = 'analyzer';
  }
  activePanel.value = 'manuscript';
}

function closeManuscriptEditor(): void {
  const target = manuscriptReturnPanel.value;
  if (target === 'reports' && !reportsCtx.currentReport.value) {
    activePanel.value = 'analyzer';
  } else {
    activePanel.value = target;
  }
}

provide(openManuscriptEditorKey, openManuscriptEditor);

const writingFilePath = ref('');
const writingChapterTitle = ref('');
const newDocLocation = ref<string | undefined>(undefined);

function openInWritingMode(filePath: string, title: string): void {
  writingFilePath.value = filePath;
  writingChapterTitle.value = title;
  appMode.value = 'writing';
}

provide('openInWritingMode', openInWritingMode);

function closeWritingDocument(): void {
  writingFilePath.value = '';
  writingChapterTitle.value = '';
}

provide('closeWritingDocument', closeWritingDocument);

function bumpFileTree(): void {
  fileTreeTick.value += 1;
}

provide('bumpFileTree', bumpFileTree);

function openNewDocumentForm(location?: string): void {
  if (!storiesCtx.activeFolder.value) return;
  panelBeforeNewDoc.value = activePanel.value;
  modeBeforeNewDoc.value = appMode.value;
  newDocLocation.value = location;
  activePanel.value = 'new-document';
}

provide('openNewDocumentForm', openNewDocumentForm);

function onDocumentCreated(path: string, title: string): void {
  fileTreeTick.value += 1;
  newDocLocation.value = undefined;
  writingFilePath.value = path;
  writingChapterTitle.value = title;
  appMode.value = 'writing';
  activePanel.value = 'analyzer';
}

function onDocumentFormCancel(): void {
  newDocLocation.value = undefined;
  activePanel.value = panelBeforeNewDoc.value;
  appMode.value = modeBeforeNewDoc.value;
}

const editingStory = ref<Story | null>(null);

function openStoryForm(story: Story | null): void {
  editingStory.value = story;
  showPanel('story-form');
}

function onStoryCreated(): void {
  openSources(true);
}

const editingSeries = ref<Series | null>(null);
const editingCampaignId = ref<number | null>(null);

function openCampaignDetail(id: number): void {
  editingCampaignId.value = id;
  showPanel('campaign-detail');
}

function openCampaignForm(id: number | null): void {
  editingCampaignId.value = id;
  showPanel('campaign-form');
}

function onCampaignSaved(id: number): void {
  editingCampaignId.value = id;
  showPanel('campaign-detail');
}

function onCampaignFormCancel(): void {
  if (editingCampaignId.value) {
    showPanel('campaign-detail');
  } else {
    showPanel('campaigns');
  }
}

function onCampaignDetailBack(): void {
  editingCampaignId.value = null;
  showPanel('campaigns');
}

function openSeriesForm(series: Series | null): void {
  editingSeries.value = series;
  showPanel('series');
}

watch(() => storiesCtx.activeStoryId.value, (id) => {
  if (id && storiesCtx.activeFolder.value) {
    analysisCtx.refreshState(storiesCtx.activeFolder.value);
    reportsCtx.loadSidebarReports(storiesCtx.activeFolder.value, platformCtx.platform.value);
    void campaignsCtx.loadCampaigns(storiesCtx.activeFolder.value);
    void campaignsCtx.loadLandingPages(storiesCtx.activeFolder.value);
  } else {
    analysisCtx.refreshState('');
    reportsCtx.loadSidebarReports('', platformCtx.platform.value);
    campaignsCtx.campaigns.value = [];
    campaignsCtx.landingPages.value = [];
  }
});

watch(() => platformCtx.platform.value, () => {
  if (storiesCtx.activeFolder.value && platformCtx.platform.value !== 'saved') {
    reportsCtx.loadSidebarReports(storiesCtx.activeFolder.value, platformCtx.platform.value);
  }
});

watch(() => analysisCtx.isWorking.value, (working, wasWorking) => {
  if (wasWorking && !working && storiesCtx.activeFolder.value) {
    analysisCtx.refreshState(storiesCtx.activeFolder.value);
    reportsCtx.loadSidebarReports(storiesCtx.activeFolder.value, platformCtx.platform.value);
  }
});

onMounted(() => {
  void loadReportTypes({ force: true });
  void checkHealth();
  void storiesCtx.loadStories().then(() => {
    const folder = storiesCtx.activeFolder.value;
    if (folder) {
      analysisCtx.refreshState(folder);
      reportsCtx.loadSidebarReports(folder, platformCtx.platform.value);
    }
  });
  void seriesCtx.loadSeries();
  void campaignsCtx.loadPlatformAccounts();
});
</script>

<template>
  <div id="app-root" :class="{ 'sidebar-drawer-open': sidebarOpen }" :style="appRootStyle">
    <MaintenanceScreen v-if="maintenanceMode" />
    <div
      v-if="sidebarOpen"
      class="sidebar-backdrop"
      aria-hidden="true"
      @click="closeSidebar"
    />
    <TitleBar />
    <div class="sidebar-column">
      <Sidebar @open-story-form="openStoryForm" @open-series-form="openSeriesForm" />
      <div
        class="sidebar-resizer"
        title="Drag to resize"
        @mousedown="startSidebarResize"
      />
    </div>
    <main id="main">
      <NewDocumentForm
        v-if="activePanel === 'new-document'"
        :initial-location="newDocLocation"
        @created="onDocumentCreated"
        @cancel="onDocumentFormCancel"
      />

      <StorySourcesPanel
        v-else-if="activePanel === 'sources'"
        :wizard="sourcesWizard"
      />

      <WritingPanel
        v-else-if="appMode === 'writing'"
        :file-path="writingFilePath"
        :chapter-title="writingChapterTitle"
        :story-folder="storiesCtx.activeFolder.value"
      />

      <template v-else-if="appMode === 'marketing'">
        <CampaignsPanel
          v-if="activePanel === 'campaigns'"
          @open-campaign="openCampaignDetail"
          @new-campaign="openCampaignForm(null)"
          @platform-accounts="showPanel('platform-accounts')"
        />
        <CampaignDetailPanel
          v-else-if="activePanel === 'campaign-detail' && editingCampaignId"
          :campaign-id="editingCampaignId"
          @back="onCampaignDetailBack"
          @edit="openCampaignForm(editingCampaignId)"
        />
        <CampaignForm
          v-else-if="activePanel === 'campaign-form'"
          :campaign-id="editingCampaignId"
          @saved="onCampaignSaved"
          @cancel="onCampaignFormCancel"
        />
        <PlatformAccountsPanel
          v-else-if="activePanel === 'platform-accounts'"
          @back="showPanel('campaigns')"
        />
      </template>

      <template v-else-if="appMode === 'analyzer'">
        <SavedReportsPanel
          v-if="activePanel === 'analyzer' && platformCtx.platform.value === 'saved'"
        />
        <AnalyzerPanel v-else-if="activePanel === 'analyzer'" />
        <ReportsViewer v-if="activePanel === 'reports'" />
        <AdminPanel v-if="activePanel === 'admin'" />
        <SettingsPanel v-if="activePanel === 'settings'" />
        <HelpPanel v-if="activePanel === 'help'" />
        <StoryForm
          v-if="activePanel === 'story-form'"
          :story="editingStory"
          @story-created="onStoryCreated"
        />
        <SeriesForm v-if="activePanel === 'series'" :series="editingSeries" />
        <ManuscriptViewer
          v-if="activePanel === 'manuscript'"
          :findings="manuscriptFindings"
          :start-index="manuscriptStartIndex"
          :story-folder="storiesCtx.activeFolder.value"
          @close="closeManuscriptEditor"
        />
      </template>
    </main>
    <StatusFooter />
  </div>
</template>

<style scoped>
#app-root {
  display: grid;
  grid-template-rows: var(--titlebar-h, 28px) 1fr auto;
  grid-template-columns: auto minmax(0, 1fr);
  grid-template-areas:
    "titlebar titlebar"
    "sidebar main"
    "footer footer";
  height: 100vh;
  overflow: hidden;
}

.sidebar-column {
  grid-area: sidebar;
  display: flex;
  width: var(--sidebar-w, 220px);
  min-width: 0;
  overflow: hidden;
}

.sidebar-column :deep(#sidebar) {
  flex: 1;
  min-width: 0;
  width: auto;
}

.sidebar-resizer {
  flex-shrink: 0;
  width: 5px;
  margin-right: -2px;
  cursor: col-resize;
  background: transparent;
  z-index: 5;
}

.sidebar-resizer:hover,
.sidebar-resizer:active {
  background: var(--accent, #4a9eff);
  opacity: 0.5;
}

#main {
  grid-area: main;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
  width: 100%;
}

#main > * {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
}

.sidebar-backdrop {
  display: none;
}

@media (max-width: 900px) {
  #app-root {
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas:
      "titlebar"
      "main";
  }

  .sidebar-column {
    position: fixed;
    top: var(--titlebar-h, 28px);
    left: 0;
    bottom: 0;
    width: min(280px, 88vw);
    z-index: 200;
    transform: translateX(-105%);
    transition: transform 0.2s ease;
    box-shadow: none;
  }

  #app-root.sidebar-drawer-open .sidebar-column {
    transform: translateX(0);
    box-shadow: 4px 0 24px var(--color-shadow);
  }

  .sidebar-resizer {
    display: none;
  }

  .sidebar-backdrop {
    display: block;
    position: fixed;
    inset: var(--titlebar-h, 28px) 0 0 0;
    z-index: 199;
    background: var(--color-backdrop);
  }
}
</style>
