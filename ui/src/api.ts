/** Drop-in replacement for Tauri invoke + event listen. */

import type { DocumentMeta, ManuscriptKind } from './types';

const BYPASS_STORAGE_KEY = 'loremetry_admin_bypass';

let authTokenProvider: (() => Promise<string | null>) | null = null;
let onAuthRequired: (() => void) | null = null;
let appSessionActive = false;

export function setAppSessionActive(active: boolean): void {
  appSessionActive = active;
  if (!active) {
    closeSharedEventSource();
  }
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
type SseHandler = (event: { payload: string }) => void;

const sseSubscriptions = new Map<string, Set<SseHandler>>();
const sseWiredChannels = new Set<string>();
let sharedEventSource: EventSource | null = null;
let sseReconnectTimer: ReturnType<typeof setTimeout> | null = null;

function sseSubscriberCount(): number {
  let n = 0;
  for (const set of sseSubscriptions.values()) {
    n += set.size;
  }
  return n;
}

function closeSharedEventSource(): void {
  if (sseReconnectTimer) {
    clearTimeout(sseReconnectTimer);
    sseReconnectTimer = null;
  }
  sharedEventSource?.close();
  sharedEventSource = null;
  sseWiredChannels.clear();
}

function dispatchSse(channel: string, data: string): void {
  const handlers = sseSubscriptions.get(channel);
  if (!handlers) return;
  for (const h of handlers) {
    h({ payload: data });
  }
}

function wireChannelOnSource(channel: string): void {
  if (!sharedEventSource || sseWiredChannels.has(channel)) return;
  sseWiredChannels.add(channel);
  sharedEventSource.addEventListener(channel, (e: Event) => {
    const msg = e as MessageEvent;
    dispatchSse(channel, String(msg.data ?? ''));
  });
}

function wireAllChannelsOnSource(): void {
  for (const channel of sseSubscriptions.keys()) {
    wireChannelOnSource(channel);
  }
}

function scheduleSseReconnect(): void {
  if (sseReconnectTimer || sseSubscriberCount() === 0 || !appSessionActive) return;
  sseReconnectTimer = setTimeout(() => {
    sseReconnectTimer = null;
    openSharedEventSource();
  }, 4000);
}

function openSharedEventSource(): void {
  if (!appSessionActive || sseSubscriberCount() === 0) return;
  if (sharedEventSource) return;

  const es = new EventSource('/api/events');
  sharedEventSource = es;
  es.onerror = () => {
    closeSharedEventSource();
    scheduleSseReconnect();
  };
  wireAllChannelsOnSource();
}

function ensureSseSubscription(channel: string, handler: SseHandler): void {
  let set = sseSubscriptions.get(channel);
  if (!set) {
    set = new Set();
    sseSubscriptions.set(channel, set);
  }
  set.add(handler);
  if (sharedEventSource) {
    wireChannelOnSource(channel);
  } else {
    openSharedEventSource();
  }
}

function removeSseSubscription(channel: string, handler: SseHandler): void {
  const set = sseSubscriptions.get(channel);
  if (!set) return;
  set.delete(handler);
  if (set.size === 0) {
    sseSubscriptions.delete(channel);
    sseWiredChannels.delete(channel);
  }
  if (sseSubscriberCount() === 0) {
    closeSharedEventSource();
  }
}

/**
 * Subscribe to server log events (single shared SSE connection).
 * SSE format: `event: <channel>\ndata: <message>` (see crates/web/src/sse.rs).
 */
export function listen(event: string, handler: SseHandler): Unlisten {
  ensureSseSubscription(event, handler);
  return () => {
    removeSseSubscription(event, handler);
  };
}

/** Analysis log channels — connect only while a job is running (avoids idle SSE through proxies). */
let analysisLogUnsubs: Unlisten[] | null = null;

export function connectAnalysisLogStream(onLine: (message: string) => void): void {
  if (analysisLogUnsubs) return;
  const handler = (e: { payload: string }) => onLine(e.payload);
  analysisLogUnsubs = [
    listen('genre:log', handler),
    listen('cdp:log', handler),
  ];
}

export function disconnectAnalysisLogStream(): void {
  if (!analysisLogUnsubs) return;
  for (const u of analysisLogUnsubs) {
    u();
  }
  analysisLogUnsubs = null;
}

export async function uploadDocuments(
  storyId: string,
  files: FileList | File[],
  kind: ManuscriptKind = 'chapter',
  options?: { replace?: boolean },
): Promise<{ uploaded: number }> {
  assertAppSession();
  const prepared = prepareUploadFiles(files, kind);
  if (prepared.length === 0) {
    throw new Error(
      kind === 'chapter'
        ? 'No .md or .txt files found. Choose a folder of chapter files.'
        : 'No files selected.',
    );
  }

  const fd = new FormData();
  for (const { file, path } of prepared) {
    fd.append('files', file, path);
  }
  const params = new URLSearchParams({ kind });
  if (options?.replace) params.set('replace', 'true');
  const headers = await buildAuthHeaders();
  const res = await fetch(
    `/api/stories/${encodeURIComponent(storyId)}/documents/upload?${params}`,
    { method: 'POST', headers, body: fd },
  );
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error((data as { error?: string }).error || 'Upload failed');
  }
  const data = await res.json() as {
    success?: boolean;
    errors?: string[];
    documents?: unknown[];
  };
  if (data.errors?.length) {
    throw new Error(data.errors.join('; '));
  }
  return { uploaded: data.documents?.length ?? prepared.length };
}

const MANUSCRIPT_EXT = /\.(md|markdown|txt)$/i;

function relativeUploadPath(file: File): string {
  const rel = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  const raw = (rel && rel.trim()) ? rel : file.name;
  return raw.replace(/\\/g, '/');
}

function isManuscriptPath(path: string): boolean {
  const base = path.split('/').pop() || path;
  return MANUSCRIPT_EXT.test(base);
}

/** Strip the common top-level folder name from a folder upload, keep act subfolders. */
function stripCommonRootPrefix(paths: string[]): string[] {
  if (paths.length === 0) return paths;
  const parts = paths.map(p => p.split('/').filter(Boolean));
  if (parts.some(segments => segments.length < 2)) {
    return paths;
  }
  const root = parts[0][0];
  if (!parts.every(segments => segments[0] === root)) {
    return paths;
  }
  return parts.map(segments => segments.slice(1).join('/'));
}

function prepareUploadFiles(
  files: FileList | File[],
  kind: ManuscriptKind,
): { file: File; path: string }[] {
  const list = Array.from(files);
  const filtered = kind === 'chapter'
    ? list.filter(f => isManuscriptPath(relativeUploadPath(f)))
    : list;
  const paths = filtered.map(f => relativeUploadPath(f));
  const trimmed = kind === 'chapter' ? stripCommonRootPrefix(paths) : paths;
  return filtered.map((file, i) => ({ file, path: trimmed[i] }));
}

/** @deprecated Use uploadDocuments with kind 'chapter' */
export async function uploadChapters(storyId: string, files: FileList | File[]): Promise<{ uploaded: number }> {
  return uploadDocuments(storyId, files, 'chapter');
}

export async function listStoryDocuments(storyId: string): Promise<DocumentMeta[]> {
  assertAppSession();
  const headers = await buildAuthHeaders();
  const res = await fetch(`/api/stories/${encodeURIComponent(storyId)}/documents`, { headers });
  const data = await res.json() as { success?: boolean; documents?: DocumentMeta[]; error?: string };
  if (!res.ok) throw new Error(data.error || 'Could not load documents');
  return data.documents ?? [];
}

export async function deleteStoryDocument(storyId: string, docId: number): Promise<void> {
  assertAppSession();
  const headers = await buildAuthHeaders();
  const res = await fetch(
    `/api/documents/${docId}?story_id=${encodeURIComponent(storyId)}`,
    { method: 'DELETE', headers },
  );
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error((data as { error?: string }).error || 'Delete failed');
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
