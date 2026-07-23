<script setup lang="ts">
import { onMounted } from 'vue';
import AuthPage from './components/AuthPage.vue';
import MainApp from './components/MainApp.vue';
import ClerkTokenWire from './components/ClerkTokenWire.vue';
import { useAuth } from './composables/useAuth';

const props = defineProps<{
  clerkEnabled?: boolean;
  publishableKey?: string;
}>();

const auth = useAuth();

onMounted(() => {
  document.addEventListener('contextmenu', (e) => {
    const tag = (e.target as HTMLElement).tagName;
    if (!['INPUT', 'TEXTAREA', 'SELECT'].includes(tag) && !(e.target as HTMLElement).isContentEditable) {
      e.preventDefault();
    }
  });

  void auth.loadAuthConfig().then(() => auth.restoreSession());
});
</script>

<template>
  <ClerkTokenWire v-if="props.clerkEnabled" />
  <MainApp v-if="auth.enteredApp" />
  <AuthPage v-else />
</template>
