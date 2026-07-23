import { ref, computed } from 'vue';
import { getOperatorBypassToken, setAuthTokenProvider, setOperatorBypassToken } from '../api';

export type MeResponse = {
  id: string;
  email: string;
  role: string;
  isAdmin: boolean;
  breakGlass?: boolean;
};

const clerkEnabled = ref(false);
const publishableKey = ref('');
const me = ref<MeResponse | null>(null);
const authReady = ref(false);

export function useAuth() {
  const breakGlass = computed(() => me.value?.breakGlass === true);
  const isAdmin = computed(() => me.value?.isAdmin === true);
  const isSignedIn = computed(() => me.value !== null);

  async function loadAuthConfig(): Promise<void> {
    try {
      const headers = new Headers();
      const bypass = getOperatorBypassToken();
      if (bypass) {
        headers.set('X-Loremetry-Admin-Bypass', bypass);
      }
      const res = await fetch('/api/auth/config', { headers });
      const data = await res.json();
      clerkEnabled.value = Boolean(data.clerkEnabled);
      publishableKey.value = data.publishableKey ?? '';
    } catch {
      clerkEnabled.value = false;
    }
  }

  async function refreshMe(): Promise<void> {
    try {
      const headers = new Headers();
      const bypass = getOperatorBypassToken();
      if (bypass) {
        headers.set('X-Loremetry-Admin-Bypass', bypass);
      }
      const res = await fetch('/api/me', { headers });
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

  function applyOperatorBypass(token: string): void {
    setOperatorBypassToken(token);
    void refreshMe();
  }

  function clearOperatorBypass(): void {
    setOperatorBypassToken('');
    me.value = null;
    authReady.value = false;
    void refreshMe();
  }

  function wireClerkGetToken(getToken: () => Promise<string | null>): void {
    setAuthTokenProvider(async () => {
      if (getOperatorBypassToken()) {
        return null;
      }
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
    breakGlass,
    isAdmin,
    isSignedIn,
    loadAuthConfig,
    refreshMe,
    applyOperatorBypass,
    clearOperatorBypass,
    wireClerkGetToken,
  };
}
