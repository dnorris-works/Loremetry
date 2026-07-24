import { unref } from 'vue';

type ClerkGetToken = (options?: { skipCache?: boolean }) => Promise<string | null>;

/** Resolve a Clerk session JWT from @clerk/vue `useAuth()`. */
export async function resolveClerkSessionToken(clerk: {
  getToken: unknown;
}): Promise<string | null> {
  try {
    const raw = clerk.getToken as unknown;
    const fn =
      typeof raw === 'function'
        ? (raw as ClerkGetToken)
        : unref(raw as ClerkGetToken | null);
    if (typeof fn !== 'function') {
      return null;
    }
    const fresh = await fn({ skipCache: true });
    if (fresh) return fresh;
    return (await fn()) ?? null;
  } catch {
    return null;
  }
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
