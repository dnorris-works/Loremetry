<script setup lang="ts">
import { inject, ref } from 'vue';
import { settingsKey } from '../../../injectionKeys';

const settingsCtx = inject(settingsKey)!;
const status = ref('');

async function onTest(): Promise<void> {
  status.value = 'Testing…';
  const result = await settingsCtx.testDataforseo();
  status.value = result.success ? 'Connected' : result.error || 'Connection failed';
}
</script>

<template>
  <div class="settings-form">
    <p class="panel-desc">
      Keyword search volume uses <strong>DataForSEO</strong> (app.dataforseo.com).
      Credentials are stored on the server by your operator.
    </p>
    <button type="button" class="btn btn-sm" @click="onTest">Test Connection</button>
    <div class="status-msg">{{ status }}</div>
  </div>
</template>

<style scoped>
.panel-desc { font-size: 13px; color: var(--text-muted); line-height: 1.5; margin-bottom: 12px; }
.status-msg { font-size: 12px; color: var(--text-muted); margin-top: 8px; min-height: 16px; }
</style>
