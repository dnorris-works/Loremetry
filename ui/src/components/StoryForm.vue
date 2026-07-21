<script setup lang="ts">
import { inject, ref, watch } from 'vue';
import { storiesKey, showPanelKey } from '../injectionKeys';
import type { Story, StoriesResult } from '../types';

const storiesCtx = inject(storiesKey)!;
const showPanel = inject(showPanelKey)!;

const props = defineProps<{
  story: Story | null;
}>();

const name = ref('');
const biblePath = ref('');
const error = ref('');
const isEditing = ref(false);
const editId = ref('');

watch(() => props.story, (s) => {
  if (s) {
    name.value = s.name;
    biblePath.value = s.bible_path || '';
    editId.value = s.id;
    isEditing.value = true;
  } else {
    name.value = '';
    biblePath.value = '';
    editId.value = '';
    isEditing.value = false;
  }
  error.value = '';
}, { immediate: true });

async function onSave(): Promise<void> {
  const trimName = name.value.trim();
  if (!trimName) { error.value = 'Please enter a story name.'; return; }
  error.value = '';

  let result: StoriesResult;
  if (isEditing.value && editId.value) {
    result = await storiesCtx.updateStory(editId.value, trimName, biblePath.value.trim());
  } else {
    result = await storiesCtx.initStory(trimName);
  }

  if (!result.success) {
    error.value = result.error;
    return;
  }

  const saved = isEditing.value && editId.value
    ? result.stories.find(s => s.id === editId.value)
    : [...result.stories].reverse().find(s => s.name === trimName);
  if (saved) storiesCtx.setActiveStory(saved.id);
  showPanel('analyzer');
}

function onCancel(): void {
  showPanel('analyzer');
}

async function onDelete(): Promise<void> {
  if (!editId.value) return;
  if (!confirm('Remove this story? Documents and reports will also be deleted.')) return;

  const result = await storiesCtx.deleteStory(editId.value);
  if (result.success) {
    showPanel('analyzer');
  } else {
    error.value = result.error;
  }
}
</script>

<template>
  <div class="panel story-form-panel">
    <h2 class="panel-title">{{ isEditing ? 'Edit Story' : 'New Story' }}</h2>

    <div class="form-group">
      <label>Story Name</label>
      <input v-model="name" type="text" placeholder="My Novel" />
    </div>

    <div v-if="isEditing" class="form-group">
      <label>
        Story Bible note
        <span class="form-hint">
          (optional — free-text note or reference for this story)
        </span>
      </label>
      <input v-model="biblePath" type="text" placeholder="Optional" />
    </div>

    <div v-if="error" class="form-error">{{ error }}</div>

    <div class="form-actions">
      <button class="btn" @click="onSave">{{ isEditing ? 'Save' : 'Create' }}</button>
      <button class="btn btn-secondary" @click="onCancel">Cancel</button>
      <button
        v-if="isEditing"
        class="btn btn-danger"
        @click="onDelete"
      >Delete</button>
    </div>
  </div>
</template>

<style scoped>
.story-form-panel {
  padding: 20px;
  max-width: 480px;
}

.panel-title {
  font-size: 16px;
  font-weight: 700;
  margin-bottom: 16px;
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

.form-hint {
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
  font-size: 11px;
}

.form-group input[type="text"],
.form-group input:not([type]) {
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text);
  font-size: 13px;
  padding: 8px 10px;
  width: 100%;
  user-select: text;
}

.form-error {
  color: var(--danger);
  font-size: 12px;
  margin-bottom: 12px;
}

.form-actions {
  display: flex;
  gap: 8px;
  margin-top: 16px;
}

.btn {
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: #fff;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
  padding: 9px 18px;
  transition: background 0.15s;
}

.btn:hover { background: var(--accent-dim); }
.btn:disabled { background: var(--surface2); color: var(--text-muted); cursor: not-allowed; }

.btn-secondary {
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.btn-secondary:hover {
  color: var(--text);
  border-color: var(--accent);
}

.btn-danger {
  background: #c0392b;
  color: #fff;
}

.btn-danger:hover { background: #a93226; }
</style>
