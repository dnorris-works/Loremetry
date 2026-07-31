<script setup lang="ts">
import { ref, watch, onBeforeUnmount, nextTick, computed, shallowRef } from 'vue';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Highlight from '@tiptap/extension-highlight';
import { Markdown } from 'tiptap-markdown';
import { invoke } from '../api';
import { analyzeStyleCoach, type StyleCoachHighlight, type StyleCoachStats } from '../lib/styleCoach';
import { StyleCoachMarks, styleCoachPluginKey } from '../lib/styleCoachExtension';

// ── Props ─────────────────────────────────────────────────────────────────────

const props = withDefaults(
  defineProps<{
    filePath: string;
    highlightText?: string;
    /** Live readability highlights + grade in the editor. */
    styleCoach?: boolean;
  }>(),
  { styleCoach: true },
);

const emit = defineEmits<{
  (e: 'saved'): void;
}>();

// ── State ─────────────────────────────────────────────────────────────────────

const saving = ref(false);
const saveStatus = ref('');
const content = ref('');  // Original content as loaded from disk
const dirty = ref(false);
let saveTimer: ReturnType<typeof setTimeout> | null = null;
let coachTimer: ReturnType<typeof setTimeout> | null = null;

const styleCoachOn = ref(props.styleCoach);
const coachHighlights = shallowRef<StyleCoachHighlight[]>([]);
const coachStats = ref<StyleCoachStats | null>(null);

const emptyStats = (): StyleCoachStats => ({
  wordCount: 0,
  sentenceCount: 0,
  paragraphCount: 0,
  adverbCount: 0,
  passiveCount: 0,
  hardSentenceCount: 0,
  veryHardSentenceCount: 0,
  complexWordCount: 0,
  gradeLevel: 0,
  readingEase: 0,
  readingTimeMinutes: 0,
});

function refreshStyleCoach(): void {
  if (!editor.value || !props.styleCoach || !styleCoachOn.value) {
    coachHighlights.value = [];
    if (props.styleCoach) coachStats.value = emptyStats();
    return;
  }
  const text = editor.value.state.doc.textContent;
  const result = analyzeStyleCoach(text);
  coachStats.value = result.stats;
  coachHighlights.value = result.highlights;
  const view = editor.value.view;
  view.dispatch(view.state.tr.setMeta(styleCoachPluginKey, Date.now()));
}

function scheduleStyleCoach(): void {
  if (!props.styleCoach || !styleCoachOn.value) return;
  if (coachTimer) clearTimeout(coachTimer);
  coachTimer = setTimeout(() => refreshStyleCoach(), 250);
}

function toggleStyleCoach(): void {
  styleCoachOn.value = !styleCoachOn.value;
  refreshStyleCoach();
}

// ── Editor setup ──────────────────────────────────────────────────────────────

const editor = useEditor({
  extensions: [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
    }),
    Highlight.configure({ multicolor: true }),
    StyleCoachMarks.configure({
      getHighlights: () => coachHighlights.value,
      enabled: () => props.styleCoach && styleCoachOn.value,
    }),
    Markdown.configure({
      html: false,
      transformPastedText: true,
      transformCopiedText: true,
    }),
  ],
  content: '',
  editorProps: {
    attributes: {
      class: 'chapter-editor-content',
    },
    handleDOMEvents: {
      blur: () => { if (dirty.value) scheduleSave(); return false; },
      keydown: (_view: any, event: KeyboardEvent) => {
        // Cmd+D (Mac) or Ctrl+D (Win) = pin selection
        if ((event.metaKey || event.ctrlKey) && event.key === 'd') {
          event.preventDefault();
          pinSelection();
          return true;
        }
        // Escape = clear pins
        if (event.key === 'Escape' && pinnedSelections.value.length > 0) {
          event.preventDefault();
          clearPins();
          return true;
        }
        return false;
      },
    },
  },
  onUpdate: () => {
    dirty.value = true;
    scheduleStyleCoach();
  },
  onSelectionUpdate: () => {
    updateSelection();
  },
  onCreate: () => {
    void nextTick(() => refreshStyleCoach());
  },
});

function runFormat(command: () => boolean): void {
  if (!editor.value) return;
  command();
  dirty.value = true;
}

// ── Load file ─────────────────────────────────────────────────────────────────

async function loadFile(filePath: string): Promise<void> {
  if (!filePath) return;
  try {
    const text = await invoke<string>('read_chapter', { filePath });
    content.value = text;
    editor.value?.commands.setContent(text);
    await nextTick();
    await nextTick();
    refreshStyleCoach();
    if (props.highlightText) {
      highlightTarget(props.highlightText);
    }
  } catch (e) {
    editor.value?.commands.setContent(`Error loading: ${e}`);
  }
}

// ── Save to disk ──────────────────────────────────────────────────────────────

function scheduleSave(): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => saveNow(), 2000);
}

async function saveNow(): Promise<void> {
  if (!editor.value || !props.filePath || !dirty.value) return;

  const text = ((editor.value.storage as any).markdown as { getMarkdown: () => string }).getMarkdown();

  if (text === content.value) {
    dirty.value = false;
    return;
  }

  saving.value = true;
  try {
    await invoke<void>('save_chapter', {
      filePath: props.filePath,
      content: text,
    });
    content.value = text;
    dirty.value = false;
    saveStatus.value = 'Saved';
    emit('saved');
    setTimeout(() => { saveStatus.value = ''; }, 1500);
  } catch (e) {
    saveStatus.value = 'Save failed: ' + String(e);
  } finally {
    saving.value = false;
  }
}

// ── Highlight a text range ────────────────────────────────────────────────────

function highlightTarget(text: string): void {
  if (!editor.value || !text) return;

  const doc = editor.value.state.doc;
  const fullText = doc.textContent;
  const idx = fullText.indexOf(text);
  if (idx < 0) return;

  let pos = 0;
  let from = 0;
  let to = 0;
  doc.descendants((node, nodePos) => {
    if (from > 0) return false;
    if (node.isText) {
      const nodeText = node.text || '';
      const relIdx = fullText.indexOf(text, pos) - pos;
      if (relIdx >= 0 && relIdx < nodeText.length) {
        from = nodePos + relIdx;
        to = from + text.length;
        return false;
      }
      pos += nodeText.length;
    }
    return true;
  });

  if (from === 0) {
    doc.descendants((node, nodePos) => {
      if (from > 0) return false;
      if (node.isText) {
        const nodeText = node.text || '';
        const localIdx = nodeText.indexOf(text.substring(0, Math.min(20, text.length)));
        if (localIdx >= 0) {
          from = nodePos + localIdx;
          to = from + text.length;
          return false;
        }
      }
      return true;
    });
  }

  if (from > 0 && to > from) {
    editor.value.chain()
      .setTextSelection({ from, to })
      .setHighlight()
      .run();

    setTimeout(() => {
      const el = document.querySelector('.chapter-editor-content mark');
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }, 100);
  }
}

function insertAtCursor(text: string): void {
  if (!editor.value) return;
  editor.value.chain().focus().insertContent(text).run();
  dirty.value = true;
  scheduleSave();
}

function replaceSelection(text: string): void {
  if (!editor.value) return;
  const { from, to } = editor.value.state.selection;
  if (from === to) {
    insertAtCursor(text);
  } else {
    editor.value.chain().focus().deleteSelection().insertContent(text).run();
    dirty.value = true;
    scheduleSave();
  }
}

const selectedText = ref('');
const pinnedSelections = ref<string[]>([]);

function updateSelection(): void {
  if (!editor.value) { selectedText.value = ''; return; }
  const { from, to } = editor.value.state.selection;
  if (from === to) { selectedText.value = ''; return; }
  selectedText.value = editor.value.state.doc.textBetween(from, to, ' ');
}

function pinSelection(): void {
  if (!editor.value || !selectedText.value) return;
  pinnedSelections.value.push(selectedText.value);
  editor.value.chain().setHighlight().run();
}

function clearPins(): void {
  pinnedSelections.value = [];
  if (editor.value) {
    editor.value.chain().selectAll().unsetHighlight().run();
    editor.value.commands.setTextSelection(editor.value.state.doc.content.size);
  }
}

const allSelectedText = computed(() => {
  const parts = [...pinnedSelections.value];
  if (selectedText.value && !parts.includes(selectedText.value)) {
    parts.push(selectedText.value);
  }
  return parts.join('\n\n---\n\n');
});

defineExpose({ insertAtCursor, replaceSelection, highlightTarget, saveNow, dirty, selectedText: allSelectedText, pinSelection, clearPins, pinnedCount: computed(() => pinnedSelections.value.length) });

watch(() => props.filePath, (fp) => { if (fp) loadFile(fp); }, { immediate: true });

watch(() => props.highlightText, (text) => {
  if (text && editor.value) highlightTarget(text);
});

watch(() => props.styleCoach, (on) => {
  styleCoachOn.value = on;
  refreshStyleCoach();
});

onBeforeUnmount(() => {
  if (saveTimer) clearTimeout(saveTimer);
  if (coachTimer) clearTimeout(coachTimer);
});
</script>

<template>
  <div class="chapter-editor">
    <div v-if="editor" class="editor-toolbar">
      <div class="toolbar-group">
        <button type="button" class="tb-btn" title="Bold" :class="{ active: editor.isActive('bold') }" @click="runFormat(() => editor!.chain().focus().toggleBold().run())"><b>B</b></button>
        <button type="button" class="tb-btn" title="Italic" :class="{ active: editor.isActive('italic') }" @click="runFormat(() => editor!.chain().focus().toggleItalic().run())"><i>I</i></button>
        <button type="button" class="tb-btn" title="Strikethrough" :class="{ active: editor.isActive('strike') }" @click="runFormat(() => editor!.chain().focus().toggleStrike().run())"><s>S</s></button>
      </div>
      <div class="toolbar-group">
        <button type="button" class="tb-btn" title="Heading 1" :class="{ active: editor.isActive('heading', { level: 1 }) }" @click="runFormat(() => editor!.chain().focus().toggleHeading({ level: 1 }).run())">H1</button>
        <button type="button" class="tb-btn" title="Heading 2" :class="{ active: editor.isActive('heading', { level: 2 }) }" @click="runFormat(() => editor!.chain().focus().toggleHeading({ level: 2 }).run())">H2</button>
        <button type="button" class="tb-btn" title="Heading 3" :class="{ active: editor.isActive('heading', { level: 3 }) }" @click="runFormat(() => editor!.chain().focus().toggleHeading({ level: 3 }).run())">H3</button>
      </div>
      <div class="toolbar-group">
        <button type="button" class="tb-btn" title="Bullet list" :class="{ active: editor.isActive('bulletList') }" @click="runFormat(() => editor!.chain().focus().toggleBulletList().run())">• List</button>
        <button type="button" class="tb-btn" title="Numbered list" :class="{ active: editor.isActive('orderedList') }" @click="runFormat(() => editor!.chain().focus().toggleOrderedList().run())">1. List</button>
        <button type="button" class="tb-btn" title="Quote" :class="{ active: editor.isActive('blockquote') }" @click="runFormat(() => editor!.chain().focus().toggleBlockquote().run())">“”</button>
        <button type="button" class="tb-btn" title="Horizontal rule" @click="runFormat(() => editor!.chain().focus().setHorizontalRule().run())">―</button>
      </div>
      <div class="toolbar-group">
        <button type="button" class="tb-btn" title="Undo" @click="editor.chain().focus().undo().run()">↶</button>
        <button type="button" class="tb-btn" title="Redo" @click="editor.chain().focus().redo().run()">↷</button>
      </div>
      <div v-if="styleCoach" class="toolbar-group">
        <button
          type="button"
          class="tb-btn"
          :class="{ active: styleCoachOn }"
          title="Style coach (readability highlights)"
          @click="toggleStyleCoach"
        >
          Style
        </button>
      </div>
      <div class="toolbar-status">
        <span v-if="saving" class="status-saving">Saving...</span>
        <span v-else-if="saveStatus" class="status-saved">{{ saveStatus }}</span>
        <span v-else-if="dirty" class="status-dirty">Unsaved</span>
      </div>
    </div>
    <div v-if="styleCoach && styleCoachOn && coachStats" class="coach-bar">
      <div class="coach-grade">
        <span class="coach-grade-num">{{ coachStats.gradeLevel }}</span>
        <span class="coach-grade-label">Grade</span>
      </div>
      <div class="coach-metrics">
        <span>{{ coachStats.wordCount.toLocaleString() }} words</span>
        <span>{{ coachStats.readingTimeMinutes }} min read</span>
        <span>{{ coachStats.sentenceCount }} sentences</span>
      </div>
      <div class="coach-issues">
        <span class="coach-pill coach-adverb">{{ coachStats.adverbCount }} adverbs</span>
        <span class="coach-pill coach-passive">{{ coachStats.passiveCount }} passive</span>
        <span class="coach-pill coach-hard">{{ coachStats.hardSentenceCount }} hard</span>
        <span class="coach-pill coach-very-hard">{{ coachStats.veryHardSentenceCount }} very hard</span>
        <span class="coach-pill coach-complex">{{ coachStats.complexWordCount }} complex</span>
      </div>
    </div>
    <EditorContent :editor="editor" class="editor-wrapper" />
  </div>
</template>

<style scoped>
.chapter-editor {
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
}

.editor-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--surface);
  flex-shrink: 0;
}

.toolbar-group {
  display: flex;
  gap: 2px;
  padding-right: 6px;
  border-right: 1px solid var(--border);
}

.toolbar-group:last-of-type {
  border-right: none;
}

.tb-btn {
  background: none;
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 600;
  min-width: 28px;
  height: 26px;
  padding: 0 7px;
  cursor: pointer;
  line-height: 1;
}

.tb-btn:hover {
  background: var(--surface2);
  color: var(--text);
  border-color: var(--border);
}

.tb-btn.active {
  background: color-mix(in srgb, var(--accent) 18%, var(--surface2));
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
}

.toolbar-status {
  margin-left: auto;
  font-size: 11px;
  padding-right: 4px;
}

.status-saving {
  color: var(--text-muted);
  font-style: italic;
}

.status-saved {
  color: var(--color-success);
}

.status-dirty {
  color: var(--text-muted);
  font-style: italic;
}

.editor-wrapper {
  flex: 1;
  overflow-y: auto;
}

.editor-wrapper :deep(.chapter-editor-content) {
  outline: none;
  padding: 20px 24px;
  font-size: 14px;
  line-height: 1.8;
  color: var(--text);
  min-height: 100%;
  caret-color: var(--accent);
}

.editor-wrapper :deep(.chapter-editor-content)::selection,
.editor-wrapper :deep(.chapter-editor-content) *::selection {
  background: color-mix(in srgb, var(--color-accent) 25%, transparent);
}

.editor-wrapper :deep(.chapter-editor-content:not(:focus))::selection,
.editor-wrapper :deep(.chapter-editor-content:not(:focus)) *::selection {
  background: var(--color-accent-subtle);
}

.editor-wrapper :deep(.chapter-editor-content p) {
  margin: 0 0 1em;
}

.editor-wrapper :deep(.chapter-editor-content h1),
.editor-wrapper :deep(.chapter-editor-content h2),
.editor-wrapper :deep(.chapter-editor-content h3) {
  margin: 1.2em 0 0.5em;
  font-weight: 700;
  color: var(--text);
}

.editor-wrapper :deep(.chapter-editor-content h1) { font-size: 1.4em; }
.editor-wrapper :deep(.chapter-editor-content h2) { font-size: 1.2em; }
.editor-wrapper :deep(.chapter-editor-content h3) { font-size: 1.05em; }

.editor-wrapper :deep(.chapter-editor-content ul),
.editor-wrapper :deep(.chapter-editor-content ol) {
  margin: 0 0 1em;
  padding-left: 1.4em;
}

.editor-wrapper :deep(.chapter-editor-content blockquote) {
  margin: 0 0 1em;
  padding: 0.2em 0 0.2em 0.9em;
  border-left: 3px solid var(--border);
  color: var(--text-muted);
}

.editor-wrapper :deep(.chapter-editor-content mark) {
  background: var(--color-mark-highlight-bg);
  color: var(--color-danger);
  font-weight: 600;
  padding: 1px 2px;
  border-radius: 2px;
}

.editor-wrapper :deep(.chapter-editor-content hr) {
  border: none;
  border-top: 1px solid var(--border);
  margin: 1.5em 0;
}

.coach-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 20px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--border);
  background: var(--surface2);
  font-size: 12px;
  flex-shrink: 0;
}

.coach-grade {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.coach-grade-num {
  font-size: 22px;
  font-weight: 800;
  color: var(--accent);
  line-height: 1;
}

.coach-grade-label {
  font-weight: 600;
  color: var(--text-muted);
}

.coach-metrics {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  color: var(--text-muted);
}

.coach-issues {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-left: auto;
}

.coach-pill {
  padding: 2px 8px;
  border-radius: 999px;
  font-weight: 600;
  font-size: 11px;
}

.coach-pill.coach-adverb { background: color-mix(in srgb, var(--color-coach-adverb) 20%, transparent); color: var(--color-coach-adverb); }
.coach-pill.coach-passive { background: color-mix(in srgb, var(--color-success) 20%, transparent); color: var(--color-success); }
.coach-pill.coach-hard { background: color-mix(in srgb, var(--color-warning) 35%, transparent); color: var(--color-warning); }
.coach-pill.coach-very-hard { background: color-mix(in srgb, var(--color-danger) 25%, transparent); color: var(--color-danger); }
.coach-pill.coach-complex { background: color-mix(in srgb, var(--color-info) 20%, transparent); color: var(--color-info); }
</style>
