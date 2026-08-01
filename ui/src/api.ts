/** Drop-in replacement for Tauri invoke + event listen. */

import type { DocumentMeta, ManuscriptKind } from './types';
import {
  countWords,
  getCachedChapter,
  hashText,
  putCachedChapter,
  removeCachedChapter,
} from './lib/manuscriptCache';

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

/** Analysis log channels — connect only while analysis is running (avoids idle SSE through proxies). */
let analysisLogUnsubs: Unlisten[] | null = null;

export function connectAnalysisLogStream(
  onLine: (message: string) => void,
  onSummaryProgress?: (payload: import('./types').SummaryChapterProgress) => void,
): void {
  if (analysisLogUnsubs) return;
  const logHandler = (e: { payload: string }) => onLine(e.payload);
  const progressHandler = (e: { payload: string }) => {
    if (!onSummaryProgress) return;
    try {
      const data = JSON.parse(e.payload) as import('./types').SummaryChapterProgress;
      if (data?.filename) onSummaryProgress(data);
    } catch {
      /* ignore malformed progress payloads */
    }
  };
  analysisLogUnsubs = [
    listen('genre:log', logHandler),
    listen('cdp:log', logHandler),
    listen('summary:chapter-progress', progressHandler),
  ];
}

export function disconnectAnalysisLogStream(): void {
  if (!analysisLogUnsubs) return;
  for (const u of analysisLogUnsubs) {
    u();
  }
  analysisLogUnsubs = null;
}

let jobEventSource: EventSource | null = null;

/** SSE log stream for a specific background job (worker process). */
export function connectJobLogStream(
  jobId: string,
  onLine: (message: string) => void,
  onSummaryProgress?: (payload: import('./types').SummaryChapterProgress) => void,
): void {
  disconnectJobLogStream();
  const es = new EventSource(`/api/events?job_id=${encodeURIComponent(jobId)}`);
  jobEventSource = es;
  const logHandler = (e: Event) => onLine(String((e as MessageEvent).data ?? ''));
  const progressHandler = (e: Event) => {
    if (!onSummaryProgress) return;
    try {
      const data = JSON.parse(String((e as MessageEvent).data ?? '')) as import('./types').SummaryChapterProgress;
      if (data?.filename) onSummaryProgress(data);
    } catch {
      /* ignore */
    }
  };
  es.addEventListener('genre:log', logHandler);
  es.addEventListener('cdp:log', logHandler);
  es.addEventListener('summary:chapter-progress', progressHandler);
}

export function disconnectJobLogStream(): void {
  jobEventSource?.close();
  jobEventSource = null;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function getJob(jobId: string): Promise<import('./types').JobRecord> {
  assertAppSession();
  const headers = await buildAuthHeaders();
  const res = await fetch(`/api/jobs/${encodeURIComponent(jobId)}`, { headers });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    maybeNotifyAuthRequired(res.status, (data as { error?: string }).error);
    throw new Error((data as { error?: string }).error || res.statusText || 'Job request failed');
  }
  return data as import('./types').JobRecord;
}

export async function cancelJob(jobId: string): Promise<void> {
  assertAppSession();
  const headers = await buildAuthHeaders({ 'Content-Type': 'application/json' });
  const res = await fetch(`/api/jobs/${encodeURIComponent(jobId)}/cancel`, {
    method: 'POST',
    headers,
    body: '{}',
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error((data as { error?: string }).error || 'Cancel failed');
  }
}

export async function waitForJob(jobId: string): Promise<import('./types').JobRecord> {
  for (;;) {
    const job = await getJob(jobId);
    if (job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled') {
      return job;
    }
    await sleep(800);
  }
}

export interface UploadResult {
  uploaded: number;
  updated: number;
  skipped: number;
  errors: string[];
}

export type UploadFileStatus = 'pending' | 'uploading' | 'done' | 'skipped' | 'error';

export type UploadProgressEvent = {
  path: string;
  status: UploadFileStatus;
  detail?: string;
};

async function postDocumentUpload(
  storyId: string,
  items: { file: File; path: string }[],
  kind: ManuscriptKind,
  replace: boolean,
): Promise<{
  documents: { id: number; path_hint: string; title: string; action?: string }[];
  updated: number;
  skipped: number;
  errors: string[];
}> {
  const fd = new FormData();
  for (const { file, path } of items) {
    fd.append('files', file, path);
  }
  const params = new URLSearchParams({ kind });
  if (replace) params.set('replace', 'true');
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
    errors?: string[];
    updated?: number;
    skipped?: number;
    documents?: {
      id: number;
      path_hint: string;
      title: string;
      action?: string;
    }[];
  };
  return {
    documents: data.documents ?? [],
    updated: data.updated ?? 0,
    skipped: data.skipped ?? 0,
    errors: data.errors ?? [],
  };
}

export async function uploadDocuments(
  storyId: string,
  files: FileList | File[],
  kind: ManuscriptKind = 'chapter',
  options?: { replace?: boolean; onProgress?: (event: UploadProgressEvent) => void },
): Promise<UploadResult> {
  assertAppSession();
  const prepared = prepareUploadFiles(files, kind);
  if (prepared.length === 0) {
    const total = Array.from(files).length;
    throw new Error(
      kind === 'chapter'
        ? total > 0
          ? `Found ${total} file(s) but none were .md, .txt, .docx, or .zip chapter files.`
          : 'No .md, .txt, .docx, or .zip files found. Choose a folder of chapter files.'
        : 'No files selected.',
    );
  }

  const onProgress = options?.onProgress;
  const replace = options?.replace === true;

  for (const { path } of prepared) {
    onProgress?.({ path, status: 'pending' });
  }

  // Bible / reference: one batch (usually few files).
  if (kind !== 'chapter') {
    for (const { path } of prepared) {
      onProgress?.({ path, status: 'uploading' });
    }
    try {
      const result = await postDocumentUpload(storyId, prepared, kind, replace);
      if (result.errors.length && !result.documents.length) {
        throw new Error(result.errors.join('; '));
      }
      const created = result.documents.filter(d => d.action !== 'updated').length;
      for (const { path } of prepared) {
        const doc = result.documents.find(d => d.path_hint === path);
        onProgress?.({
          path,
          status: doc || result.documents.length ? 'done' : 'error',
          detail: doc?.action === 'updated' ? 'updated' : 'saved',
        });
      }
      return {
        uploaded: created,
        updated: result.updated,
        skipped: result.skipped,
        errors: result.errors,
      };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      for (const { path } of prepared) {
        onProgress?.({ path, status: 'error', detail: msg });
      }
      throw e;
    }
  }

  // Chapters: upload one at a time so the UI can turn each row green as it hits the DB.
  let uploaded = 0;
  let updated = 0;
  let skipped = 0;
  const errors: string[] = [];

  for (const item of prepared) {
    let content = '';
    try {
      content = await item.file.text();
    } catch {
      content = '';
    }
    const hash = await hashText(
      content || `${item.file.size}:${item.file.name}:${item.file.lastModified}`,
    );
    const cached = await getCachedChapter(storyId, item.path);
    if (cached?.hash === hash) {
      skipped += 1;
      onProgress?.({ path: item.path, status: 'skipped', detail: 'unchanged' });
      continue;
    }

    onProgress?.({ path: item.path, status: 'uploading' });
    try {
      const result = await postDocumentUpload(storyId, [item], kind, false);
      if (result.errors.length && !result.documents.length) {
        throw new Error(result.errors.join('; '));
      }
      errors.push(...result.errors);
      updated += result.updated;
      skipped += result.skipped;
      uploaded += result.documents.filter(d => d.action !== 'updated').length;

      const doc = result.documents.find(d => d.path_hint === item.path) ?? result.documents[0];
      const now = new Date().toISOString();
      await putCachedChapter({
        storyId,
        path: item.path,
        content,
        hash,
        wordCount: countWords(content),
        docId: doc?.id ?? null,
        updatedAt: now,
      });

      const detail = result.skipped > 0 && !doc
        ? 'unchanged'
        : doc?.action === 'updated'
          ? 'updated'
          : 'saved';
      onProgress?.({
        path: item.path,
        status: detail === 'unchanged' ? 'skipped' : 'done',
        detail,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      errors.push(`${item.path}: ${msg}`);
      onProgress?.({ path: item.path, status: 'error', detail: msg });
    }
  }

  return { uploaded, updated, skipped, errors };
}

export async function saveZeigarnikReport(
  storyId: string,
  content: string,
): Promise<void> {
  await saveClientReport('save_zeigarnik_report', storyId, content);
}

export async function saveReadabilityReport(
  storyId: string,
  content: string,
): Promise<void> {
  await saveClientReport('save_readability_report', storyId, content);
}

async function saveClientReport(
  cmd: string,
  storyId: string,
  content: string,
): Promise<void> {
  assertAppSession();
  const headers = await buildAuthHeaders({ 'Content-Type': 'application/json' });
  const res = await fetch('/api/invoke', {
    method: 'POST',
    headers,
    body: JSON.stringify({
      cmd,
      args: { request: { folder: storyId, content } },
    }),
  });
  const data = await res.json();
  if (!res.ok || (data && typeof data === 'object' && data.error)) {
    throw new Error((data as { error?: string }).error || 'Could not save report');
  }
  if (data && typeof data === 'object' && 'success' in data && !data.success) {
    throw new Error(String((data as { error?: string }).error || 'Report save failed'));
  }
}

export { removeCachedChapter };

const MANUSCRIPT_EXT = /\.(md|markdown|txt|docx)$/i;
const ZIP_EXT = /\.zip$/i;

function relativeUploadPath(file: File): string {
  const rel = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
  const raw = (rel && rel.trim()) ? rel : file.name;
  return raw.replace(/\\/g, '/');
}

function isManuscriptPath(path: string): boolean {
  const base = path.split('/').pop() || path;
  return MANUSCRIPT_EXT.test(base) || ZIP_EXT.test(base);
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
export async function uploadChapters(storyId: string, files: FileList | File[]): Promise<{ uploaded: number; skipped: number }> {
  return uploadDocuments(storyId, files, 'chapter');
}

export async function downloadStoryZip(storyId: string, storyName?: string): Promise<void> {
  assertAppSession();
  const headers = await buildAuthHeaders();
  const res = await fetch(
    `/api/stories/${encodeURIComponent(storyId)}/export.zip`,
    { headers },
  );
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error((data as { error?: string }).error || 'Export failed');
  }
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  const safe = (storyName || storyId).replace(/[^\w.-]+/g, '_');
  a.download = `${safe}.zip`;
  a.click();
  URL.revokeObjectURL(url);
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
