<script setup lang="ts">
import { onMounted, watch, unref } from 'vue';
import { useAuth as useClerkAuth } from '@clerk/vue';
import { registerClerkSignOut, useAuth } from '../composables/useAuth';
import { resolveClerkSessionToken, sleep } from '../clerkSessionToken';

const auth = useAuth();
const { breakGlass } = auth;
const clerk = useClerkAuth();

onMounted(() => {
  auth.wireClerkGetToken(() => resolveClerkSessionToken(clerk));
  registerClerkSignOut(async () => {
    try {
      const raw = clerk.signOut as unknown;
      const fn =
        typeof raw === 'function'
          ? raw
          : unref(raw as (() => Promise<void>) | null);
      if (typeof fn === 'function') {
        await fn();
      }
    } catch {
      /* ignore */
    }
  });
  void completeClerkSignIn();
});

watch(
  () => clerk.isSignedIn.value,
  (signedIn) => {
    if (signedIn && !breakGlass.value) {
      void completeClerkSignIn();
    }
  },
);

async function completeClerkSignIn(): Promise<void> {
  if (!clerk.isSignedIn.value || breakGlass.value) return;
  for (let i = 0; i < 10; i++) {
    if (await auth.refreshMe()) return;
    await sleep(350);
  }
}
</script>

<template>
  <span aria-hidden="true" class="clerk-wire" />
</template>

<style scoped>
.clerk-wire {
  display: none;
}
</style>
