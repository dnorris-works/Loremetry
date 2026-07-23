import { ref, computed } from 'vue';
import { setAuthTokenProvider } from '../api';

export type MeResponse = {
  id: string;
  email: string;
  role: string;
  isAdmin: boolean;
};

const clerkEnabled = ref(false);
const publishableKey = ref('');
const me = ref<MeResponse | null>(null);
const authReady = ref(false);

export function useAuth() {
  const isAdmin = computed(() => me.value?.isAdmin === true);
  const isSignedIn = computed(() => !clerkEnabled.value || me.value !== null);

  async function loadAuthConfig(): Promise<void> {
    try {
      const res = await fetch('/api/auth/config');
      const data = await res.json();
      clerkEnabled.value = Boolean(data.clerkEnabled);
      publishableKey.value = data.publishableKey ?? '';
    } catch {
      clerkEnabled.value = false;
    }
  }

  async function refreshMe(): Promise<void> {
    if (!clerkEnabled.value) {
      me.value = { id: '', email: 'local', role: 'admin', isAdmin: true };
      authReady.value = true;
      return;
    }
    try {
      const res = await fetch('/api/me');
      if (!res.ok) {
        me.value = null;
        return;
      }
      me.value = await res.json();
    } catch {
      me.value = null;
    } finally {
      authReady.value = true;
    }
  }

  function wireClerkGetToken(getToken: () => Promise<string | null>): void {
    setAuthTokenProvider(async () => {
      try {
        return await getToken();
      } catch {
        return null;
      }
    });
  }

  return {
    clerkEnabled,
    publishableKey,
    me,
    authReady,
    isAdmin,
    isSignedIn,
    loadAuthConfig,
    refreshMe,
    wireClerkGetToken,
  };
}
