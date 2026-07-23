<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useAuth } from '../composables/useAuth';

const { refreshMe, applyOperatorBypass, breakGlass } = useAuth();

const operatorToken = ref('');
const operatorError = ref('');

onMounted(() => {
  void refreshMe();
});

function onSubmit(): void {
  operatorError.value = '';
  const t = operatorToken.value.trim();
  if (!t) {
    operatorError.value = 'Enter the operator bypass token (set in platform credentials).';
    return;
  }
  applyOperatorBypass(t);
  void refreshMe().then(() => {
    if (!breakGlass.value) {
      operatorError.value = 'Invalid operator token or Clerk is required for normal users.';
    }
  });
}
</script>

<template>
  <div class="auth-screen">
    <h1>Loremetry</h1>
    <p class="auth-hint">Clerk is not configured. Operator bypass is required.</p>
    <form class="operator-form" @submit.prevent="onSubmit">
      <label class="field-label">Operator bypass token</label>
      <input
        v-model="operatorToken"
        type="password"
        autocomplete="off"
        spellcheck="false"
        class="cred-input"
      />
      <button type="submit" class="btn">Continue</button>
      <p v-if="operatorError" class="operator-error">{{ operatorError }}</p>
    </form>
  </div>
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

.operator-form {
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
