<script setup lang="ts">
import { inject } from 'vue';
import { useAuth } from '../composables/useAuth';

const auth = useAuth();
const toggleSidebar = inject<() => void>('toggleSidebar');

function onSignOut(): void {
  void auth.signOut();
}
</script>

<template>
  <header id="titlebar">
    <div class="titlebar-left">
      <button
        type="button"
        class="titlebar-menu"
        aria-label="Open menu"
        @click="toggleSidebar?.()"
      >
        ☰
      </button>
      <span class="titlebar-label">Loremetry</span>
    </div>
    <button type="button" class="titlebar-btn" @click="onSignOut">Sign out</button>
  </header>
</template>

<style scoped>
#titlebar {
  grid-area: titlebar;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  gap: 12px;
  user-select: none;
  -webkit-user-select: none;
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.titlebar-menu {
  display: none;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 24px;
  padding: 0;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--text);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
}

.titlebar-menu:hover {
  border-color: var(--accent);
}

@media (max-width: 900px) {
  .titlebar-menu {
    display: inline-flex;
  }
}

.titlebar-label {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.02em;
  color: var(--text-muted);
}

.titlebar-btn {
  font-size: 12px;
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface-raised, var(--surface));
  color: var(--text);
  cursor: pointer;
  flex-shrink: 0;
}

.titlebar-btn:hover {
  border-color: var(--accent, #6b8cff);
}
</style>
