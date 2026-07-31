<script setup lang="ts">
import { inject } from 'vue';
import { settingsKey } from '../../../injectionKeys';

const settingsCtx = inject(settingsKey)!;
</script>

<template>
  <div class="settings-form">
    <p class="panel-desc">Appearance and display preferences for this browser.</p>
    <label class="field-label">Theme</label>
    <div class="provider-options">
      <label class="provider-option" :class="{ active: settingsCtx.theme.value === 'dark' }">
        <input
          type="radio"
          name="theme"
          value="dark"
          :checked="settingsCtx.theme.value === 'dark'"
          @change="settingsCtx.setTheme('dark')"
        />
        Dark
      </label>
      <label class="provider-option" :class="{ active: settingsCtx.theme.value === 'light' }">
        <input
          type="radio"
          name="theme"
          value="light"
          :checked="settingsCtx.theme.value === 'light'"
          @change="settingsCtx.setTheme('light')"
        />
        Light
      </label>
    </div>
  </div>
</template>

<style scoped>
.panel-desc {
  font-size: 13px;
  color: var(--text-muted);
  line-height: 1.5;
  margin-bottom: 12px;
}

.field-label {
  font-size: 12px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 8px;
  display: block;
}

.provider-options {
  display: flex;
  gap: 16px;
}

.provider-option {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
}

.provider-option input[type="radio"] {
  accent-color: var(--accent);
}

.provider-option.active {
  color: var(--accent);
}
</style>
