<script setup lang="ts">
import { ref } from 'vue';
import { useAuth } from '../composables/useAuth';

const auth = useAuth();

const showPassphrase = ref(false);
const passphrase = ref('');
const passphraseError = ref('');

function openPassphrase(): void {
  passphraseError.value = '';
  showPassphrase.value = true;
}

function closePassphrase(): void {
  showPassphrase.value = false;
  passphrase.value = '';
  passphraseError.value = '';
}

function submitPassphrase(): void {
  passphraseError.value = '';
  const t = passphrase.value.trim();
  if (!t) {
    passphraseError.value = 'Enter the operator bypass token from server logs or Admin.';
    return;
  }
  auth.applyOperatorBypass(t);
  void auth.refreshMe().then(() => {
    if (auth.breakGlass.value) {
      closePassphrase();
    } else {
      passphraseError.value = 'Invalid token.';
    }
  });
}

function signOutOperator(): void {
  auth.clearOperatorBypass();
  closePassphrase();
}
</script>

<template>
  <header id="titlebar">
    <span class="titlebar-label">Loremetry</span>
    <div class="titlebar-actions">
      <button
        v-if="auth.breakGlass.value"
        type="button"
        class="titlebar-btn titlebar-btn-active"
        title="Operator mode active"
        @click="signOutOperator"
      >
        Operator mode · Sign out
      </button>
      <button
        v-else
        type="button"
        class="titlebar-btn"
        title="Full access without Clerk (token from deploy logs or Admin)"
        @click="openPassphrase"
      >
        Enter passphrase
      </button>
    </div>
  </header>

  <div v-if="showPassphrase" class="passphrase-backdrop" @click.self="closePassphrase">
    <form class="passphrase-dialog" @submit.prevent="submitPassphrase">
      <h2>Operator passphrase</h2>
      <p class="passphrase-hint">
        Use the bypass token from deploy logs (first boot) or Admin → Platform credentials. Unlocks the full app and Admin without Clerk.
      </p>
      <input
        v-model="passphrase"
        type="password"
        autocomplete="off"
        spellcheck="false"
        class="passphrase-input"
        placeholder="Bypass token"
      />
      <p v-if="passphraseError" class="passphrase-error">{{ passphraseError }}</p>
      <div class="passphrase-actions">
        <button type="button" class="btn btn-sm" @click="closePassphrase">Cancel</button>
        <button type="submit" class="btn btn-sm">Unlock</button>
      </div>
    </form>
  </div>
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

.titlebar-actions {
  margin-left: auto;
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

.titlebar-btn-active {
  border-color: var(--accent, #6b8cff);
  color: var(--accent, #6b8cff);
}

.passphrase-backdrop {
  position: fixed;
  inset: 0;
  z-index: 10000;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.passphrase-dialog {
  width: 100%;
  max-width: 420px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 20px;
}

.passphrase-dialog h2 {
  margin: 0 0 8px;
  font-size: 1.1rem;
}

.passphrase-hint {
  font-size: 0.85rem;
  color: var(--text-muted);
  margin: 0 0 12px;
  line-height: 1.4;
}

.passphrase-input {
  width: 100%;
  box-sizing: border-box;
  margin-bottom: 8px;
}

.passphrase-error {
  color: var(--danger, #c44);
  font-size: 0.85rem;
  margin: 0 0 8px;
}

.passphrase-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 12px;
}
</style>
