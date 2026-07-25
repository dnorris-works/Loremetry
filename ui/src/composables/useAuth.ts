import { ref, computed } from 'vue';
import {
  buildAuthHeaders,
  getOperatorBypassToken,
  setAuthTokenProvider,
  setOperatorBypassToken,
  registerAuthRequiredHandler,
  setAppSessionActive,
} from '../api';
import { isDesktopApp } from '../platform';

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
/** Only true after session probe succeeds — gates the main app. */
const enteredApp = ref(false);
const restoringSession = ref(false);
const sessionError = ref('');

let clerkSignOut: (() => Promise<void>) | null = null;

registerAuthRequiredHandler(() => {
  setOperatorBypassToken('');
  me.value = null;
  enteredApp.value = false;
  setAppSessionActive(false);
});

export function registerClerkSignOut(fn: () => Promise<void>): void {
  clerkSignOut = fn;
}

/** Call from App with boot-time `/api/auth/config` (before async reload). */
export function syncBootClerkConfig(enabled?: boolean, key?: string): void {
  if (enabled) clerkEnabled.value = true;
  if (key) publishableKey.value = key;
}

/** Validate session with the server; sets `enteredApp` only on success. */
async function refreshMe(): Promise<boolean> {
  sessionError.value = '';
  const bypass = getOperatorBypassToken();
  const headers = await buildAuthHeaders();
  const hasAuth = bypass || headers.has('Authorization');
  if (!hasAuth) {
    me.value = null;
    enteredApp.value = false;
    setAppSessionActive(false);
    return false;
  }

  try {
    const res = await fetch('/api/auth/session', { headers });
    if (!res.ok) {
      sessionError.value = 'Could not verify session with the server.';
      me.value = null;
      enteredApp.value = false;
      setAppSessionActive(false);
      return false;
    }
    const data = (await res.json()) as { authenticated?: boolean; reason?: string } & Partial<MeResponse>;
    if (!data.authenticated) {
      if (bypass) {
        setOperatorBypassToken('');
      }
      sessionError.value = data.reason?.trim()
        || 'Sign-in could not be verified. Check Clerk JWT issuer and publishable key in Admin → Platform credentials.';
      me.value = null;
      enteredApp.value = false;
      setAppSessionActive(false);
      return false;
    }
    me.value = {
      id: String(data.id ?? ''),
      email: data.email ?? '',
      role: data.role ?? '',
      isAdmin: Boolean(data.isAdmin),
      breakGlass: data.breakGlass,
    };
    enteredApp.value = true;
    setAppSessionActive(true);
    sessionError.value = '';
    return true;
  } catch {
    sessionError.value = 'Could not reach the server.';
    me.value = null;
    enteredApp.value = false;
    setAppSessionActive(false);
    return false;
  }
}

export function useAuth() {
  const breakGlass = computed(() => me.value?.breakGlass === true);
  const isAdmin = computed(() => me.value?.isAdmin === true);
  const isSignedIn = computed(() => enteredApp.value);

  async function loadAuthConfig(): Promise<void> {
    if (isDesktopApp()) {
      clerkEnabled.value = false;
      return;
    }
    try {
      const res = await fetch('/api/auth/config');
      const data = await res.json();
      clerkEnabled.value = Boolean(data.clerkEnabled);
      publishableKey.value = data.publishableKey ?? '';
    } catch {
      clerkEnabled.value = false;
    }
  }

  async function restoreSession(): Promise<void> {
    if (isDesktopApp()) {
      enteredApp.value = true;
      setAppSessionActive(true);
      return;
    }
    restoringSession.value = true;
    try {
      await refreshMe();
    } finally {
      restoringSession.value = false;
    }
  }

  async function applyOperatorBypass(token: string): Promise<boolean> {
    setOperatorBypassToken(token.trim());
    return refreshMe();
  }

  function clearOperatorBypass(): void {
    setOperatorBypassToken('');
    me.value = null;
    enteredApp.value = false;
    setAppSessionActive(false);
  }

  async function signOut(): Promise<void> {
    sessionError.value = '';
    if (breakGlass.value) {
      setOperatorBypassToken('');
    } else if (clerkSignOut) {
      await clerkSignOut();
    }
    me.value = null;
    enteredApp.value = false;
    setAppSessionActive(false);
  }

  function wireClerkGetToken(getToken: () => Promise<string | null>): void {
    const provider = async (): Promise<string | null> => {
      if (getOperatorBypassToken()) {
        return null;
      }
      try {
        return await getToken();
      } catch {
        return null;
      }
    };
    setAuthTokenProvider(provider);
  }

  return {
    clerkEnabled,
    publishableKey,
    me,
    enteredApp,
    restoringSession,
    sessionError,
    breakGlass,
    isAdmin,
    isSignedIn,
    loadAuthConfig,
    refreshMe,
    restoreSession,
    applyOperatorBypass,
    clearOperatorBypass,
    signOut,
    wireClerkGetToken,
  };
}
