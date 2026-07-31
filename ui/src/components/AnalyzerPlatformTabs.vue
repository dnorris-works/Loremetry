<script setup lang="ts">
import { usePlatform } from '../composables/usePlatform';

const platformCtx = usePlatform();
</script>

<template>
  <div class="platform-tabs">
    <button
      class="platform-tab"
      :class="{ active: platformCtx.platform.value === 'kdp' }"
      @click="platformCtx.setPlatform('kdp')"
    >KDP/Wide</button>
    <button
      class="platform-tab"
      :class="{ active: platformCtx.platform.value === 'craft' }"
      @click="platformCtx.setPlatform('craft')"
    >Craft</button>
    <button
      class="platform-tab"
      :class="{ active: platformCtx.platform.value === 'publish' }"
      @click="platformCtx.setPlatform('publish')"
    >Publish</button>
    <button
      class="platform-tab"
      :class="{ active: platformCtx.platform.value === 'saved' }"
      @click="platformCtx.setPlatform('saved')"
    >Saved Reports</button>
  </div>
</template>

<style scoped>
.platform-tabs {
  display: flex;
  gap: 0;
  margin-bottom: 14px;
  border-bottom: 2px solid var(--border);
}

.platform-tab {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 600;
  padding: 8px 16px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -2px;
  transition: color 0.15s, border-color 0.15s;
}

.platform-tab:hover {
  color: var(--text);
}

.platform-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}
</style>
