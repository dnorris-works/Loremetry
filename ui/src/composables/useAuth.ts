import { ref, computed } from 'vue';
import {
  buildAuthHeaders,
  getOperatorBypassToken,
  setAuthTokenProvider,
  setOperatorBypassToken,
  registerAuthRequiredHandler,
  setAppSessionActive,
} from '../api';

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

let authTokenProvider: (() => Promise<string | null>) | null = null;
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

async function hasClerkBearer(): Promise<boolean> {
  if (!authTokenProvider) return false;
  try {
    const t = await authTokenProvider();
    return Boolean(t);
  } catch {
    return false;
  }
}

/** Validate session with the server; sets `enteredApp` only on success. */
async function refreshMe(): Promise<boolean> {
  const bypass = getOperatorBypassToken();
  const bearer = await hasClerkBearer();
  if (!bypass && !bearer) {
    me.value = null;
    enteredApp.value = false;
    setAppSessionActive(false);
    return false;
  }

  try {
    const headers = await buildAuthHeaders();
    const res = await fetch('/api/auth/session', { headers });
    if (!res.ok) {
      me.value = null;
      enteredApp.value = false;
      setAppSessionActive(false);
      return false;
    }
    const data = (await res.json()) as { authenticated?: boolean } & Partial<MeResponse>;
    if (!data.authenticated) {
      if (bypass) {
        setOperatorBypassToken('');
      }
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
    return true;
  } catch {
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
    authTokenProvider = provider;
    setAuthTokenProvider(provider);
  }

  return {
    clerkEnabled,
    publishableKey,
    me,
    enteredApp,
    restoringSession,
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
