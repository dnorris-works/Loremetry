<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue';
import {
  uploadDocuments,
  listStoryDocuments,
  deleteStoryDocument,
  removeCachedChapter,
  downloadStoryZip,
} from '../api';
import { storiesKey, showPanelKey, analysisKey } from '../injectionKeys';
import type { DocumentMeta, ManuscriptKind } from '../types';

const props = defineProps<{
  /** First-time setup wizard after creating a story. */
  wizard?: boolean;
}>();

const storiesCtx = inject(storiesKey)!;
const showPanel = inject(showPanelKey)!;
const analysisCtx = inject(analysisKey)!;
const bumpFileTree = inject<() => void>('bumpFileTree', () => {});

const loading = ref(false);
const uploading = ref(false);
const uploadMessage = ref('');
const importErrors = ref<string[]>([]);
const error = ref('');
const documents = ref<DocumentMeta[]>([]);

const wizardStep = ref(0);

const chapters = computed(() =>
  documents.value.filter(d => d.kind === 'chapter').sort(compareDocs),
);
const bibles = computed(() =>
  documents.value.filter(d => d.kind === 'bible').sort(compareDocs),
);
const characters = computed(() =>
  documents.value.filter(d => d.kind === 'character').sort(compareDocs),
);
const locations = computed(() =>
  documents.value.filter(d => d.kind === 'location').sort(compareDocs),
);
const referenceCount = computed(() => characters.value.length + locations.value.length);

const hasChapters = computed(() => chapters.value.length > 0);

const wizardSteps = ['Chapters', 'Story bible', 'Reference'];
const isWizard = computed(() => props.wizard === true);

function compareDocs(a: DocumentMeta, b: DocumentMeta): number {
  return a.path_hint.localeCompare(b.path_hint, undefined, { numeric: true });
}

function displayName(doc: DocumentMeta): string {
  return doc.path_hint || doc.title || `document-${doc.id}`;
}

function chapterLabel(doc: DocumentMeta): string {
  const parts = doc.path_hint.split('/');
  if (parts.length > 1) {
    return doc.path_hint;
  }
  return displayName(doc);
}

async function refresh(): Promise<void> {
  const storyId = storiesCtx.activeFolder.value;
  if (!storyId) {
    documents.value = [];
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    documents.value = await listStoryDocuments(storyId);
    analysisCtx.refreshState(storyId);
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

watch(() => storiesCtx.activeFolder.value, () => {
  wizardStep.value = 0;
  void refresh();
}, { immediate: true });

async function onUpload(
  ev: Event,
  kind: ManuscriptKind,
  replace = false,
): Promise<void> {
  const input = ev.target as HTMLInputElement;
  const files = input.files;
  input.value = '';
  const storyId = storiesCtx.activeFolder.value;
  if (!files?.length || !storyId) return;

  uploading.value = true;
  error.value = '';
  uploadMessage.value = '';
  importErrors.value = [];
  try {
    const { uploaded, updated, skipped, errors } = await uploadDocuments(storyId, files, kind, { replace });
    const parts: string[] = [];
    if (uploaded > 0) {
      parts.push(`${uploaded} new`);
    }
    if (updated > 0) {
      parts.push(`${updated} updated`);
    }
    if (skipped > 0) {
      parts.push(`${skipped} unchanged`);
    }
    uploadMessage.value = parts.length ? `Import: ${parts.join(', ')}.` : 'No changes to upload.';
    importErrors.value = errors;
    await refresh();
    bumpFileTree();
  } catch (e) {
    error.value = String(e);
  } finally {
    uploading.value = false;
  }
}

async function onDelete(doc: DocumentMeta): Promise<void> {
  const storyId = storiesCtx.activeFolder.value;
  if (!storyId) return;
  const label = displayName(doc);
  if (!confirm(`Remove “${label}”?`)) return;

  error.value = '';
  try {
    await deleteStoryDocument(storyId, doc.id);
    if (doc.kind === 'chapter' && doc.path_hint) {
      await removeCachedChapter(storyId, doc.path_hint);
    }
    await refresh();
    bumpFileTree();
  } catch (e) {
    error.value = String(e);
  }
}

function wizardNext(): void {
  if (wizardStep.value < wizardSteps.length - 1) {
    wizardStep.value += 1;
    return;
  }
  finishWizard();
}

function wizardBack(): void {
  if (wizardStep.value > 0) wizardStep.value -= 1;
}

function finishWizard(): void {
  showPanel('analyzer');
}

async function onDownloadZip(): Promise<void> {
  const storyId = storiesCtx.activeFolder.value;
  const story = storiesCtx.activeStory.value;
  if (!storyId) return;
  error.value = '';
  try {
    await downloadStoryZip(storyId, story?.name);
  } catch (e) {
    error.value = String(e);
  }
}
</script>

<template>
  <div class="panel sources-panel">
    <header class="sources-header">
      <div>
        <h2 class="panel-title">
          {{ isWizard ? 'Set up your story' : 'Story sources' }}
        </h2>
        <p class="panel-desc">
          <template v-if="storiesCtx.activeStory.value">
            {{ storiesCtx.activeStory.value.name }} —
          </template>
          <template v-if="isWizard">
            Add your manuscript files so analysis can run.
          </template>
          <template v-else>
            Chapters are required. Bible and reference files improve craft reports.
          </template>
        </p>
      </div>
      <div v-if="!isWizard" class="sources-summary">
        <span :class="{ ok: hasChapters }">{{ chapters.length }} chapters</span>
        <span>{{ bibles.length }} bible</span>
        <span>{{ referenceCount }} reference</span>
        <button type="button" class="btn btn-secondary btn-sm" :disabled="!hasChapters" @click="onDownloadZip">
          Download zip
        </button>
      </div>
    </header>

    <div v-if="isWizard" class="wizard-steps">
      <div
        v-for="(label, i) in wizardSteps"
        :key="label"
        class="wizard-step"
        :class="{ active: wizardStep === i, done: wizardStep > i }"
      >
        <span class="wizard-step-num">{{ i + 1 }}</span>
        <span class="wizard-step-label">{{ label }}</span>
      </div>
    </div>

    <div v-if="loading" class="sources-loading">Loading…</div>

    <div v-else class="sources-body">
      <!-- Wizard step 1 / Manage: Chapters -->
      <section
        v-show="!isWizard || wizardStep === 0"
        class="source-section"
      >
        <div class="section-head">
          <h3 class="section-title">Chapters <span class="required">required</span></h3>
          <div class="section-actions">
            <label class="btn btn-secondary btn-sm upload-btn">
              {{ uploading ? 'Uploading…' : 'Choose folder' }}
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                webkitdirectory
                multiple
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'chapter')"
              />
            </label>
            <label class="btn btn-secondary btn-sm upload-btn">
              Add files
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                multiple
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'chapter')"
              />
            </label>
          </div>
        </div>
        <p class="section-hint">
          Point at the folder where your chapter files live (<code>.md</code>, <code>.docx</code>, or a <code>.zip</code> export).
          Subfolders (e.g. <code>Act-1/</code>) are kept for order. Re-upload merges changed files only.
        </p>
        <ul v-if="chapters.length" class="doc-list">
          <li v-for="doc in chapters" :key="doc.id" class="doc-row">
            <span class="doc-name" :title="doc.path_hint">{{ chapterLabel(doc) }}</span>
            <button class="doc-delete" title="Remove" @click="onDelete(doc)">&times;</button>
          </li>
        </ul>
        <p v-else class="section-empty">No chapters yet — upload at least one to run analysis.</p>
      </section>

      <!-- Wizard step 2 / Manage: Bible -->
      <section
        v-show="!isWizard || wizardStep === 1"
        class="source-section"
      >
        <div class="section-head">
          <h3 class="section-title">Story bible <span class="optional">optional</span></h3>
          <div class="section-actions">
            <label class="btn btn-secondary btn-sm upload-btn">
              {{ bibles.length ? 'Replace bible' : 'Upload bible' }}
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'bible', true)"
              />
            </label>
            <label v-if="!bibles.length" class="btn btn-secondary btn-sm upload-btn">
              Add another
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'bible')"
              />
            </label>
          </div>
        </div>
        <p class="section-hint">
          World rules, canon, tone — used for summaries, continuity, and craft reports.
          Skip if this lives in your chapter files only.
        </p>
        <ul v-if="bibles.length" class="doc-list">
          <li v-for="doc in bibles" :key="doc.id" class="doc-row">
            <span class="doc-name" :title="doc.path_hint">{{ displayName(doc) }}</span>
            <button class="doc-delete" title="Remove" @click="onDelete(doc)">&times;</button>
          </li>
        </ul>
        <p v-else class="section-empty">No story bible uploaded.</p>
      </section>

      <!-- Wizard step 3 / Manage: Reference -->
      <section
        v-show="!isWizard || wizardStep === 2"
        class="source-section"
      >
        <div class="section-head">
          <h3 class="section-title">Reference <span class="optional">optional</span></h3>
        </div>
        <p class="section-hint">
          Character profiles and location notes — merged with your bible for AI context.
          Skip if already covered in your bible.
        </p>

        <div class="ref-subsection">
          <div class="ref-sub-head">
            <span class="ref-label">Character profiles</span>
            <label class="btn btn-secondary btn-sm upload-btn">
              Add files
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                multiple
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'character')"
              />
            </label>
          </div>
          <ul v-if="characters.length" class="doc-list">
            <li v-for="doc in characters" :key="doc.id" class="doc-row">
              <span class="doc-name" :title="doc.path_hint">{{ displayName(doc) }}</span>
              <button class="doc-delete" title="Remove" @click="onDelete(doc)">&times;</button>
            </li>
          </ul>
          <p v-else class="section-empty compact">None</p>
        </div>

        <div class="ref-subsection">
          <div class="ref-sub-head">
            <span class="ref-label">Location notes</span>
            <label class="btn btn-secondary btn-sm upload-btn">
              Add files
              <input
                type="file"
                accept=".md,.txt,.docx,.zip,text/markdown,text/plain,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/zip"
                multiple
                hidden
                :disabled="uploading"
                @change="onUpload($event, 'location')"
              />
            </label>
          </div>
          <ul v-if="locations.length" class="doc-list">
            <li v-for="doc in locations" :key="doc.id" class="doc-row">
              <span class="doc-name" :title="doc.path_hint">{{ displayName(doc) }}</span>
              <button class="doc-delete" title="Remove" @click="onDelete(doc)">&times;</button>
            </li>
          </ul>
          <p v-else class="section-empty compact">None</p>
        </div>
      </section>
    </div>

    <div v-if="uploadMessage" class="upload-message">{{ uploadMessage }}</div>
    <ul v-if="importErrors.length" class="import-errors">
      <li v-for="(msg, i) in importErrors" :key="i">{{ msg }}</li>
    </ul>
    <div v-if="error" class="form-error">{{ error }}</div>

    <footer class="sources-footer">
      <template v-if="isWizard">
        <button
          v-if="wizardStep > 0"
          class="btn btn-secondary"
          @click="wizardBack"
        >Back</button>
        <button
          v-if="wizardStep > 0 && wizardStep < 2"
          class="btn btn-secondary"
          @click="wizardNext"
        >Skip</button>
        <button
          v-if="wizardStep < 2"
          class="btn"
          :disabled="wizardStep === 0 && !hasChapters"
          @click="wizardNext"
        >Next</button>
        <button
          v-else
          class="btn"
          :disabled="!hasChapters"
          @click="finishWizard"
        >Done</button>
      </template>
      <template v-else>
        <button class="btn" :disabled="!hasChapters" @click="showPanel('analyzer')">
          Go to Analyzer
        </button>
      </template>
    </footer>
  </div>
</template>

<style scoped>
.sources-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: var(--content-pad, 20px);
  overflow: hidden;
  min-width: 0;
}

.sources-header {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin-bottom: 6px;
}

.panel-desc {
  color: var(--text-muted);
  font-size: 13px;
  line-height: 1.5;
  margin: 0;
}

.sources-summary {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
  align-items: flex-start;
}

.sources-summary .ok {
  color: var(--accent);
  font-weight: 600;
}

.wizard-steps {
  display: flex;
  gap: 8px;
  margin-bottom: 20px;
  flex-wrap: wrap;
}

.wizard-step {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-radius: var(--radius);
  background: var(--surface);
  border: 1px solid var(--border);
  font-size: 12px;
  color: var(--text-muted);
}

.wizard-step.active {
  border-color: var(--accent);
  color: var(--text);
}

.wizard-step.done {
  opacity: 0.7;
}

.wizard-step-num {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--surface2);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  font-size: 11px;
}

.wizard-step.active .wizard-step-num {
  background: var(--accent);
  color: var(--color-on-accent);
}

.sources-loading {
  color: var(--text-muted);
  font-size: 13px;
}

.source-section {
  flex: 0 0 auto;
  margin-bottom: 20px;
  padding-bottom: 20px;
  border-bottom: 1px solid var(--border);
  max-width: 640px;
}

.source-section:last-of-type {
  border-bottom: none;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 6px;
}

.section-title {
  font-size: 14px;
  font-weight: 700;
  margin: 0;
}

.required {
  font-size: 10px;
  font-weight: 600;
  color: var(--accent);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.optional {
  font-size: 10px;
  font-weight: 500;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.section-hint {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.5;
  margin: 0 0 10px;
}

.section-actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.section-empty {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
  font-style: italic;
}

.section-empty.compact {
  margin-top: 4px;
}

.doc-list {
  list-style: none;
  margin: 0;
  padding: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
}

.doc-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  font-size: 12px;
  border-bottom: 1px solid var(--border);
}

.doc-row:last-child {
  border-bottom: none;
}

.doc-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.doc-delete {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
  padding: 0 4px;
  flex-shrink: 0;
}

.doc-delete:hover {
  color: var(--danger);
}

.ref-subsection {
  margin-top: 12px;
}

.ref-sub-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}

.ref-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}

.upload-btn {
  cursor: pointer;
  margin: 0;
}

.btn-sm {
  font-size: 11px;
  padding: 6px 12px;
}

.section-hint code {
  font-size: 11px;
  background: var(--surface2);
  padding: 1px 4px;
  border-radius: 3px;
}

.upload-message {
  color: var(--accent);
  font-size: 12px;
  margin-top: 8px;
}

.import-errors {
  margin: 8px 0 0;
  padding: 8px 12px;
  background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border-radius: var(--radius);
  font-size: 12px;
  color: var(--danger);
  list-style: disc inside;
}

.form-error {
  color: var(--danger);
  font-size: 12px;
  margin-top: 8px;
}

.sources-footer {
  display: flex;
  gap: 8px;
  margin-top: auto;
  padding-top: 16px;
  flex-wrap: wrap;
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
  transition: background 0.15s;
}

.btn:hover:not(:disabled) { background: var(--accent-dim); }

.btn:disabled {
  background: var(--surface2);
  color: var(--text-muted);
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.btn-secondary:hover:not(:disabled) {
  color: var(--text);
  border-color: var(--accent);
}

/* Scrollable body between header and footer */
.sources-body {
  overflow-y: auto;
  flex: 1 1 auto;
  min-height: 0;
}
</style>
