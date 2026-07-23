<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { ClerkLoaded, ClerkLoading, Show, SignIn, useAuth as useClerkAuth } from '@clerk/vue';
import { registerClerkSignOut, useAuth } from '../composables/useAuth';

const auth = useAuth();
const clerk = useClerkAuth();

onMounted(() => {
  auth.wireClerkGetToken(async () => {
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
  registerClerkSignOut(async () => {
    if (clerk.signOut.value) {
      await clerk.signOut.value();
    }
  });
});

watch(
  () => clerk.isSignedIn.value,
  (signedIn) => {
    if (signedIn && !auth.breakGlass.value) {
      void auth.refreshMe();
    }
  },
  { immediate: true },
);
</script>

<template>
  <ClerkLoading>
    <p class="auth-muted">Loading sign-in…</p>
  </ClerkLoading>
  <ClerkLoaded>
    <Show when="signed-out">
      <p class="auth-lead">Sign in to continue.</p>
      <SignIn routing="hash" />
    </Show>
  </ClerkLoaded>
</template>

<style scoped>
.auth-lead {
  text-align: center;
  color: var(--text-muted);
  margin: 0 0 16px;
  font-size: 0.95rem;
}

.auth-muted {
  text-align: center;
  color: var(--text-muted);
  font-size: 0.85rem;
}
</style>
