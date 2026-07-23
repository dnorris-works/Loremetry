<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { useAuth as useClerkAuth } from '@clerk/vue';
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
);
</script>

<template>
  <span aria-hidden="true" class="clerk-wire" />
</template>

<style scoped>
.clerk-wire {
  display: none;
}
</style>
