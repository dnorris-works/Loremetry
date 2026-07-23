/** Drop-in replacement for Tauri invoke + event listen. */

const BYPASS_STORAGE_KEY = 'loremetry_admin_bypass';

let authTokenProvider: (() => Promise<string | null>) | null = null;
let onAuthRequired: (() => void) | null = null;
let appSessionActive = false;

export function setAppSessionActive(active: boolean): void {
  appSessionActive = active;
}

function assertAppSession(): void {
  if (!appSessionActive) {
    throw new Error('Not signed in');
  }
}

export function registerAuthRequiredHandler(fn: () => void): void {
  onAuthRequired = fn;
}

function maybeNotifyAuthRequired(status: number, errorText: string | undefined): void {
  const msg = (errorText ?? '').toLowerCase();
  const authRelated =
    status === 401
    || msg.includes('clerk is not configured')
    || msg.includes('missing authorization')
    || msg.includes('jwt invalid')
    || msg.includes('operator access required');
    if (authRelated) {
    onAuthRequired?.();
  }
}

/** True when the API indicates the session is not valid (show login page). */
export function isAuthFailureMessage(message: string): boolean {
  const msg = message.toLowerCase();
  return (
    msg.includes('clerk is not configured')
    || msg.includes('missing authorization')
    || msg.includes('jwt invalid')
    || msg.includes('operator access required')
  );
}

export function getOperatorBypassToken(): string {
  try {
    return sessionStorage.getItem(BYPASS_STORAGE_KEY)?.trim() ?? '';
  } catch {
    return '';
  }
}

export function setOperatorBypassToken(token: string): void {
  try {
    const t = token.trim();
    if (t) {
      sessionStorage.setItem(BYPASS_STORAGE_KEY, t);
    } else {
      sessionStorage.removeItem(BYPASS_STORAGE_KEY);
    }
  } catch {
    /* ignore */
  }
}

export function setAuthTokenProvider(fn: () => Promise<string | null>): void {
  authTokenProvider = fn;
}

export async function buildAuthHeaders(extra?: HeadersInit): Promise<Headers> {
  const headers = new Headers(extra);
  const bypass = getOperatorBypassToken();
  if (bypass) {
    headers.set('X-Loremetry-Admin-Bypass', bypass);
  }
  if (authTokenProvider) {
    const token = await authTokenProvider();
    if (token) {
      headers.set('Authorization', `Bearer ${token}`);
    }
  }
  return headers;
}

export async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  assertAppSession();
  const headers = await buildAuthHeaders({ 'Content-Type': 'application/json' });
  const res = await fetch('/api/invoke', {
    method: 'POST',
    headers,
    body: JSON.stringify({ cmd, args: args ?? {} }),
  });
  const data = await res.json();
  if (!res.ok) {
    maybeNotifyAuthRequired(res.status, data?.error);
    throw new Error(data?.error || res.statusText || 'Request failed');
  }
  // Error payloads are { error: string } without a success field (command results use success + error)
  if (data && typeof data === 'object' && 'error' in data && data.error && !('success' in data)) {
    maybeNotifyAuthRequired(res.status, String(data.error));
    throw new Error(String(data.error));
  }
  return data as T;
}

type Unlisten = () => void;

/**
 * Subscribe to server log events.
 * SSE format: `event: <channel>\ndata: <message>` (see crates/web/src/sse.rs).
 */
export function listen(event: string, handler: (event: { payload: string }) => void): Promise<Unlisten> {
  const es = new EventSource('/api/events');
  const onMsg = (e: MessageEvent) => {
    handler({ payload: e.data });
  };
  es.addEventListener(event, onMsg as EventListener);
  return Promise.resolve(() => es.close());
}

export async function uploadChapters(storyId: string, files: FileList | File[]): Promise<void> {
  assertAppSession();
  const fd = new FormData();
  for (const f of Array.from(files)) {
    fd.append('files', f, f.name);
  }
  const headers = await buildAuthHeaders();
  const res = await fetch(`/api/stories/${encodeURIComponent(storyId)}/documents/upload`, {
    method: 'POST',
    headers,
    body: fd,
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error((data as { error?: string }).error || 'Upload failed');
  }
}

/** Fetch for `/api/admin/*` routes. */
export async function adminFetch<T = unknown>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  assertAppSession();
  let res: Response;
  try {
    const headers = await buildAuthHeaders(init.headers);
    res = await fetch(`/api/admin${path}`, { ...init, headers });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    throw new Error(
      msg === 'Failed to fetch'
        ? 'Could not reach the server. Check that loremetry-web is running and the Admin API is deployed.'
        : msg,
    );
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    maybeNotifyAuthRequired(res.status, (data as { error?: string }).error);
    throw new Error((data as { error?: string }).error || res.statusText || 'Admin request failed');
  }
  if (data && typeof data === 'object' && 'error' in data && data.error && !('success' in data)) {
    throw new Error(String((data as { error: string }).error));
  }
  return data as T;
}

/** Upload a file to an admin multipart endpoint (for large CSVs). */
export async function adminUploadFile<T = unknown>(
  path: string,
  file: File,
  fieldName = 'file',
): Promise<T> {
  assertAppSession();
  const fd = new FormData();
  fd.append(fieldName, file, file.name);
  const headers = await buildAuthHeaders();
  const res = await fetch(`/api/admin${path}`, { method: 'POST', headers, body: fd });
  const data = await res.json();
  if (!res.ok) {
    throw new Error((data as { error?: string }).error || res.statusText || 'Upload failed');
  }
  if (data && typeof data === 'object' && 'error' in data && data.error && !('success' in data)) {
    throw new Error(String((data as { error: string }).error));
  }
  return data as T;
}
