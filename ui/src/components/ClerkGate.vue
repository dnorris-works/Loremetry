<script setup lang="ts">
import { ClerkLoaded, ClerkLoading, Show, SignIn, useAuth as useClerkAuth } from '@clerk/vue';
import { onMounted, watch } from 'vue';
import { useAuth } from '../composables/useAuth';

const { wireClerkGetToken, refreshMe } = useAuth();
const clerk = useClerkAuth();

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
});

watch(
  () => clerk.isSignedIn.value,
  (signedIn) => {
    if (signedIn) {
      void refreshMe();
    }
  },
  { immediate: true },
);
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
</style>
