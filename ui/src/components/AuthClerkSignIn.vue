<script setup lang="ts">
import { watch } from 'vue';
import { ClerkLoaded, ClerkLoading, Show, SignIn, useAuth as useClerkAuth } from '@clerk/vue';
import { useAuth } from '../composables/useAuth';
import { sleep } from '../clerkSessionToken';

const auth = useAuth();
const { enteredApp, sessionError, breakGlass } = auth;
const clerk = useClerkAuth();

watch(
  () => clerk.isSignedIn.value,
  (signedIn) => {
    if (signedIn && !breakGlass.value) {
      void (async () => {
        for (let i = 0; i < 10; i++) {
          if (await auth.refreshMe()) return;
          await sleep(350);
        }
      })();
    }
  },
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
    <Show when="signed-in">
      <p v-if="!enteredApp" class="auth-muted">
        Finishing sign-in…
      </p>
      <p v-if="sessionError" class="auth-error">{{ sessionError }}</p>
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

.auth-error {
  text-align: center;
  color: var(--danger, #c44);
  font-size: 0.85rem;
  margin: 12px 0 0;
  line-height: 1.45;
}
</style>
