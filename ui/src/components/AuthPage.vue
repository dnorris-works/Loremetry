<script setup lang="ts">
import AuthClerkSignIn from './AuthClerkSignIn.vue';
import AuthOperatorForm from './AuthOperatorForm.vue';

defineProps<{
  clerkEnabled: boolean;
}>();
</script>

<template>
  <div class="auth-page">
    <div class="auth-page-inner">
      <h1 class="auth-title">Loremetry</h1>

      <template v-if="clerkEnabled">
        <AuthClerkSignIn />
        <hr class="auth-divider" />
      </template>
      <p v-else class="auth-lead">Clerk is not configured. Use operator access to continue.</p>

      <AuthOperatorForm />
    </div>
  </div>
</template>

<style scoped>
.auth-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: var(--bg, #0f0f12);
}

.auth-page-inner {
  width: 100%;
  max-width: 440px;
}

.auth-title {
  text-align: center;
  font-size: 1.75rem;
  font-weight: 700;
  margin: 0 0 20px;
}

.auth-lead {
  text-align: center;
  color: var(--text-muted);
  margin: 0 0 20px;
  font-size: 0.95rem;
}

.auth-divider {
  border: none;
  border-top: 1px solid var(--border);
  margin: 28px 0;
}
</style>
