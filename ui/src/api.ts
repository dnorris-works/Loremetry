/** Drop-in replacement for Tauri invoke + event listen. */

export async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const res = await fetch('/api/invoke', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cmd, args: args ?? {} }),
  });
  const data = await res.json();
  if (!res.ok) {
    throw new Error(data?.error || res.statusText || 'Request failed');
  }
  // Error payloads are { error: string } without a success field (command results use success + error)
  if (data && typeof data === 'object' && 'error' in data && data.error && !('success' in data)) {
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
  const fd = new FormData();
  for (const f of Array.from(files)) {
    fd.append('files', f, f.name);
  }
  const res = await fetch(`/api/stories/${encodeURIComponent(storyId)}/documents/upload`, {
    method: 'POST',
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
  let res: Response;
  try {
    res = await fetch(`/api/admin${path}`, init);
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
  const fd = new FormData();
  fd.append(fieldName, file, file.name);
  const res = await fetch(`/api/admin${path}`, { method: 'POST', body: fd });
  const data = await res.json();
  if (!res.ok) {
    throw new Error((data as { error?: string }).error || res.statusText || 'Upload failed');
  }
  if (data && typeof data === 'object' && 'error' in data && data.error && !('success' in data)) {
    throw new Error(String((data as { error: string }).error));
  }
  return data as T;
}
