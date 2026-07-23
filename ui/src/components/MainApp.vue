<script setup lang="ts">
import { ref, watch, onMounted, provide } from 'vue';
import { useStories } from '../composables/useStories';
import { useAnalysis } from '../composables/useAnalysis';
import { usePlatform } from '../composables/usePlatform';
import { useSettings } from '../composables/useSettings';
import { useReports } from '../composables/useReports';
import { useSeries } from '../composables/useSeries';
import {
  storiesKey, analysisKey, platformKey, settingsKey,
  reportsKey, seriesKey, showPanelKey, openManuscriptEditorKey,
} from '../injectionKeys';
import type { Story, Finding, Series } from '../types';

import TitleBar from './TitleBar.vue';
import Sidebar from './Sidebar.vue';
import AnalyzerPanel from './AnalyzerPanel.vue';
import ReportsViewer from './ReportsViewer.vue';
import AdminPanel from './AdminPanel.vue';
import StoryForm from './StoryForm.vue';
import SeriesForm from './SeriesForm.vue';
import NewDocumentForm from './NewDocumentForm.vue';
import ManuscriptViewer from './ManuscriptViewer.vue';
import WritingPanel from './WritingPanel.vue';
import { useAuth } from '../composables/useAuth';
import { useReportTypes } from '../composables/useReportTypes';

const auth = useAuth();
provide('isAdmin', auth.isAdmin);

const { loadReportTypes } = useReportTypes();

const storiesCtx = useStories();
const analysisCtx = useAnalysis();
const platformCtx = usePlatform();
const settingsCtx = useSettings();
const reportsCtx = useReports();
const seriesCtx = useSeries();

provide(storiesKey, storiesCtx);
provide(analysisKey, analysisCtx);
provide(platformKey, platformCtx);
provide(settingsKey, settingsCtx);
provide(reportsKey, reportsCtx);
provide(seriesKey, seriesCtx);

type AppMode = 'analyzer' | 'writing';
const appMode = ref<AppMode>('analyzer');

provide('appMode', appMode);
provide('setAppMode', (mode: AppMode) => { appMode.value = mode; });

type Panel = 'analyzer' | 'reports' | 'admin' | 'story-form' | 'series' | 'manuscript' | 'new-document';
const activePanel = ref<Panel>('analyzer');
const panelBeforeNewDoc = ref<Panel>('analyzer');
const modeBeforeNewDoc = ref<AppMode>('analyzer');

function showPanel(name: Panel): void {
  activePanel.value = name;
}

provide(showPanelKey, showPanel as (name: string) => void);

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

const editingSeries = ref<Series | null>(null);

function openSeriesForm(series: Series | null): void {
  editingSeries.value = series;
  showPanel('series');
}

watch(() => storiesCtx.activeStoryId.value, (id) => {
  if (id && storiesCtx.activeFolder.value) {
    analysisCtx.refreshState(storiesCtx.activeFolder.value);
    reportsCtx.loadSidebarReports(storiesCtx.activeFolder.value, platformCtx.platform.value);
  } else {
    analysisCtx.refreshState('');
    reportsCtx.loadSidebarReports('', platformCtx.platform.value);
  }
});

watch(() => platformCtx.platform.value, () => {
  if (storiesCtx.activeFolder.value) {
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
  void storiesCtx.loadStories().then(() => {
    const folder = storiesCtx.activeFolder.value;
    if (folder) {
      analysisCtx.refreshState(folder);
      reportsCtx.loadSidebarReports(folder, platformCtx.platform.value);
    }
  });
  void seriesCtx.loadSeries();
});
</script>

<template>
  <div id="app-root">
    <TitleBar />
    <Sidebar @open-story-form="openStoryForm" @open-series-form="openSeriesForm" />
    <main id="main">
      <NewDocumentForm
        v-if="activePanel === 'new-document'"
        :initial-location="newDocLocation"
        @created="onDocumentCreated"
        @cancel="onDocumentFormCancel"
      />

      <WritingPanel
        v-else-if="appMode === 'writing'"
        :file-path="writingFilePath"
        :chapter-title="writingChapterTitle"
        :story-folder="storiesCtx.activeFolder.value"
      />

      <template v-else-if="appMode === 'analyzer'">
        <AnalyzerPanel v-if="activePanel === 'analyzer'" />
        <ReportsViewer v-if="activePanel === 'reports'" />
        <AdminPanel v-if="activePanel === 'admin'" />
        <StoryForm v-if="activePanel === 'story-form'" :story="editingStory" />
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
  </div>
</template>

<style scoped>
#app-root {
  display: grid;
  grid-template-rows: var(--titlebar-h, 28px) 1fr;
  grid-template-columns: 200px 1fr;
  grid-template-areas:
    "titlebar titlebar"
    "sidebar main";
  height: 100vh;
  overflow: hidden;
}

#main {
  grid-area: main;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
