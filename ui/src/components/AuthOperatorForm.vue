<script setup lang="ts">
import { ref } from 'vue';
import { useAuth } from '../composables/useAuth';

const auth = useAuth();
const operatorToken = ref('');
const operatorError = ref('');
const submitting = ref(false);

async function onOperatorSubmit(): Promise<void> {
  operatorError.value = '';
  const t = operatorToken.value.trim();
  if (!t) {
    operatorError.value = 'Enter the operator bypass token.';
    return;
  }
  submitting.value = true;
  try {
    const ok = await auth.applyOperatorBypass(t);
    if (!ok) {
      operatorError.value = 'Invalid operator token.';
    }
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <section class="operator-block">
    <h2 class="operator-heading">Operator access</h2>
    <p class="auth-muted">
      Bypass token is printed in server logs on first boot. If you lost it: set
      <code>LOREMETRY_RESET_OPERATOR_BYPASS=true</code> on the host, redeploy once, copy the new token from logs, then remove that env.
    </p>
    <form class="operator-form" @submit.prevent="onOperatorSubmit">
      <input
        v-model="operatorToken"
        type="password"
        autocomplete="off"
        spellcheck="false"
        class="operator-input"
        placeholder="Operator bypass token"
        :disabled="submitting"
      />
      <button type="submit" class="btn" :disabled="submitting">
        {{ submitting ? 'Signing in…' : 'Continue as operator' }}
      </button>
      <p v-if="operatorError" class="operator-error">{{ operatorError }}</p>
    </form>
  </section>
</template>

<style scoped>
.operator-heading {
  font-size: 1rem;
  margin: 0 0 8px;
}

.auth-muted {
  color: var(--text-muted);
  font-size: 0.85rem;
  line-height: 1.45;
  margin: 0 0 12px;
}

.operator-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.operator-input {
  width: 100%;
  box-sizing: border-box;
}

.operator-error {
  color: var(--danger, #c44);
  font-size: 0.85rem;
  margin: 0;
}
</style>
