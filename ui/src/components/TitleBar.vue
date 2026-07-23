<script setup lang="ts">
import { useAuth } from '../composables/useAuth';

const auth = useAuth();

function onSignOut(): void {
  void auth.signOut();
}
</script>

<template>
  <header id="titlebar">
    <span class="titlebar-label">Loremetry</span>
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
}

.titlebar-btn:hover {
  border-color: var(--accent, #6b8cff);
}
</style>
