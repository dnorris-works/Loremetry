<script setup lang="ts">
import { ClerkLoaded, ClerkLoading, Show, SignIn, useAuth as useClerkAuth } from '@clerk/vue';
import { onMounted, ref, watch } from 'vue';
import { useAuth } from '../composables/useAuth';

const { wireClerkGetToken, refreshMe, applyOperatorBypass, breakGlass } = useAuth();
const clerk = useClerkAuth();

const showOperator = ref(false);
const operatorToken = ref('');
const operatorError = ref('');

onMounted(() => {
  wireClerkGetToken(async () => {
    try {
      const fn = clerk.getToken.value;
      if (typeof fn === 'function') {
        return (await fn()) ?? null;
      }
      return null;
    } catch {
      return null;
    }
  });
  void refreshMe();
});

watch(
  () => clerk.isSignedIn.value,
  (signedIn) => {
    if (signedIn && !breakGlass.value) {
      void refreshMe();
    }
  },
  { immediate: true },
);

function onOperatorSubmit(): void {
  operatorError.value = '';
  const t = operatorToken.value.trim();
  if (!t) {
    operatorError.value = 'Enter the operator bypass token.';
    return;
  }
  applyOperatorBypass(t);
  void refreshMe().then(() => {
    if (!breakGlass.value) {
      operatorError.value = 'Invalid operator token.';
    }
  });
}
</script>

<template>
  <ClerkLoading>
    <p class="auth-loading">Loading sign-in…</p>
  </ClerkLoading>
  <ClerkLoaded>
    <Show when="signed-out">
      <div class="auth-screen">
        <h1>Loremetry</h1>
        <p class="auth-hint">Sign in to continue.</p>
        <SignIn routing="hash" />
        <p class="auth-operator-toggle">
          <button type="button" class="link-btn" @click="showOperator = !showOperator">
            {{ showOperator ? 'Hide operator access' : 'Operator access' }}
          </button>
        </p>
        <form v-if="showOperator" class="operator-form" @submit.prevent="onOperatorSubmit">
          <label class="field-label">Bypass token</label>
          <input
            v-model="operatorToken"
            type="password"
            autocomplete="off"
            spellcheck="false"
            class="cred-input"
          />
          <button type="submit" class="btn btn-sm">Unlock</button>
          <p v-if="operatorError" class="operator-error">{{ operatorError }}</p>
        </form>
      </div>
    </Show>
    <Show when="signed-in">
      <slot />
    </Show>
  </ClerkLoaded>
</template>

<style scoped>
.auth-screen {
  max-width: 420px;
  margin: 48px auto;
  padding: 24px;
  text-align: center;
}

.auth-hint {
  color: var(--text-muted);
  margin-bottom: 16px;
}

.auth-loading {
  text-align: center;
  padding: 48px;
  color: var(--text-muted);
}

.auth-operator-toggle {
  margin-top: 24px;
  font-size: 0.9rem;
}

.link-btn {
  background: none;
  border: none;
  color: var(--accent, #6b8cff);
  cursor: pointer;
  text-decoration: underline;
}

.operator-form {
  margin-top: 12px;
  text-align: left;
}

.operator-error {
  color: var(--danger, #c44);
  font-size: 0.85rem;
  margin-top: 8px;
}

.field-label {
  display: block;
  font-size: 0.85rem;
  margin-bottom: 4px;
}

.cred-input {
  width: 100%;
  margin-bottom: 8px;
}
</style>
