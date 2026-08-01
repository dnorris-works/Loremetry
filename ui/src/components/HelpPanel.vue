<script setup lang="ts">
import { computed, inject } from 'vue';
import { showPanelKey } from '../injectionKeys';
import helpMarkdown from '../help/reports.md?raw';

const showPanel = inject(showPanelKey)!;

function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function inlineFormat(text: string): string {
  return esc(text)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
}

function markdownToHtml(md: string): string {
  const lines = md.replace(/\r\n/g, '\n').split('\n');
  const out: string[] = [];
  let listTag: '' | 'ul' | 'ol' = '';
  let inTable = false;

  const closeList = () => {
    if (listTag) {
      out.push(`</${listTag}>`);
      listTag = '';
    }
  };
  const closeTable = () => {
    if (inTable) {
      out.push('</tbody></table>');
      inTable = false;
    }
  };
  const openList = (tag: 'ul' | 'ol') => {
    if (listTag !== tag) {
      closeList();
      out.push(`<${tag}>`);
      listTag = tag;
    }
  };

  for (const line of lines) {
    const trimmed = line.trim();

    if (trimmed.startsWith('|') && trimmed.endsWith('|')) {
      closeList();
      const cells = trimmed.slice(1, -1).split('|').map(c => c.trim());
      if (cells.every(c => /^:?-+:?$/.test(c))) continue;
      if (!inTable) {
        out.push('<table class="help-table"><thead><tr>');
        for (const c of cells) out.push(`<th>${inlineFormat(c)}</th>`);
        out.push('</tr></thead><tbody>');
        inTable = true;
      } else {
        out.push('<tr>');
        for (const c of cells) out.push(`<td>${inlineFormat(c)}</td>`);
        out.push('</tr>');
      }
      continue;
    }

    closeTable();

    if (trimmed === '---') {
      closeList();
      out.push('<hr />');
      continue;
    }
    if (trimmed.startsWith('### ')) {
      closeList();
      out.push(`<h3>${inlineFormat(trimmed.slice(4))}</h3>`);
      continue;
    }
    if (trimmed.startsWith('## ')) {
      closeList();
      out.push(`<h2>${inlineFormat(trimmed.slice(3))}</h2>`);
      continue;
    }
    if (trimmed.startsWith('# ')) {
      closeList();
      out.push(`<h1>${inlineFormat(trimmed.slice(2))}</h1>`);
      continue;
    }
    if (/^\d+\.\s/.test(trimmed)) {
      openList('ol');
      out.push(`<li>${inlineFormat(trimmed.replace(/^\d+\.\s/, ''))}</li>`);
      continue;
    }
    if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
      openList('ul');
      out.push(`<li>${inlineFormat(trimmed.slice(2))}</li>`);
      continue;
    }
    if (trimmed === '') {
      closeList();
      continue;
    }
    closeList();
    out.push(`<p>${inlineFormat(trimmed)}</p>`);
  }
  closeList();
  closeTable();
  return out.join('\n');
}

const html = computed(() => markdownToHtml(helpMarkdown));
</script>

<template>
  <div class="help-root">
    <header class="help-header">
      <h2 class="help-title">Help</h2>
      <button type="button" class="btn btn-secondary" @click="showPanel('analyzer')">Close</button>
    </header>

    <div class="help-scroll">
      <div class="help-body report-markdown" v-html="html" />
    </div>
  </div>
</template>

<style scoped>
.help-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.help-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 24px;
  background: var(--bg);
  border-bottom: 1px solid var(--border);
}

.help-title {
  font-size: 16px;
  font-weight: 700;
  margin: 0;
}

.help-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.help-body {
  padding: 16px 24px 40px;
  max-width: 720px;
  line-height: 1.55;
  font-size: 14px;
}

.help-body :deep(h1) {
  font-size: 22px;
  margin: 0 0 16px;
}

.help-body :deep(h2) {
  font-size: 17px;
  margin: 28px 0 10px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--border);
}

.help-body :deep(h3) {
  font-size: 15px;
  margin: 18px 0 6px;
}

.help-body :deep(p),
.help-body :deep(li) {
  color: var(--text);
}

.help-body :deep(p) {
  margin: 0 0 10px;
}

.help-body :deep(ul),
.help-body :deep(ol) {
  margin: 0 0 12px;
  padding-left: 1.4em;
}

.help-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border);
  margin: 20px 0;
}

.help-body :deep(code) {
  font-size: 0.92em;
  padding: 1px 5px;
  border-radius: 3px;
  background: var(--surface2);
}

.help-body :deep(.help-table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0 0 16px;
  font-size: 13px;
}

.help-body :deep(.help-table th),
.help-body :deep(.help-table td) {
  border: 1px solid var(--border);
  padding: 6px 10px;
  text-align: left;
  vertical-align: top;
}

.help-body :deep(.help-table th) {
  background: var(--surface);
}
</style>
