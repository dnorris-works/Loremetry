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

const { enteredApp, loadAuthConfig, restoreSession } = useAuth();

onMounted(() => {
  document.addEventListener('contextmenu', (e) => {
    const tag = (e.target as HTMLElement).tagName;
    if (!['INPUT', 'TEXTAREA', 'SELECT'].includes(tag) && !(e.target as HTMLElement).isContentEditable) {
      e.preventDefault();
    }
  });

  void loadAuthConfig().then(() => restoreSession());
});
</script>

<template>
  <ClerkTokenWire v-if="props.clerkEnabled" />
  <MainApp v-if="enteredApp" />
  <AuthPage v-else />
</template>
