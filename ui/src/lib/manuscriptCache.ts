/** IndexedDB cache of chapter content + hashes for delta upload and client-side analysis. */

const DB_NAME = 'loremetry-manuscript-cache';
const DB_VERSION = 1;
const STORE = 'chapters';

export interface CachedChapter {
  storyId: string;
  path: string;
  content: string;
  hash: string;
  wordCount: number;
  docId: number | null;
  updatedAt: string;
}

function cacheKey(storyId: string, path: string): string {
  return `${storyId}\0${path}`;
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onerror = () => reject(req.error ?? new Error('IndexedDB open failed'));
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE);
      }
    };
    req.onsuccess = () => resolve(req.result);
  });
}

function withStore<T>(
  mode: IDBTransactionMode,
  fn: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return openDb().then(db => new Promise<T>((resolve, reject) => {
    const tx = db.transaction(STORE, mode);
    const store = tx.objectStore(STORE);
    const req = fn(store);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('IndexedDB request failed'));
    tx.oncomplete = () => db.close();
    tx.onerror = () => reject(tx.error ?? new Error('IndexedDB transaction failed'));
  }));
}

export async function hashText(text: string): Promise<string> {
  const buf = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest('SHA-256', buf);
  return Array.from(new Uint8Array(digest))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

export function countWords(text: string): number {
  const trimmed = text.trim();
  if (!trimmed) return 0;
  return trimmed.split(/\s+/).length;
}

export async function getCachedChapter(
  storyId: string,
  path: string,
): Promise<CachedChapter | null> {
  return withStore('readonly', store =>
    store.get(cacheKey(storyId, path)),
  ).then(v => (v as CachedChapter | undefined) ?? null);
}

export async function putCachedChapter(entry: CachedChapter): Promise<void> {
  await withStore('readwrite', store =>
    store.put(entry, cacheKey(entry.storyId, entry.path)),
  );
}

export async function removeCachedChapter(storyId: string, path: string): Promise<void> {
  await withStore('readwrite', store =>
    store.delete(cacheKey(storyId, path)),
  );
}

export async function listCachedChapters(storyId: string): Promise<CachedChapter[]> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const out: CachedChapter[] = [];
    const tx = db.transaction(STORE, 'readonly');
    const store = tx.objectStore(STORE);
    const req = store.openCursor();
    req.onsuccess = () => {
      const cursor = req.result;
      if (!cursor) return;
      const entry = cursor.value as CachedChapter;
      if (entry.storyId === storyId) {
        out.push(entry);
      }
      cursor.continue();
    };
    req.onerror = () => reject(req.error ?? new Error('IndexedDB cursor failed'));
    tx.oncomplete = () => {
      db.close();
      out.sort((a, b) => a.path.localeCompare(b.path, undefined, { numeric: true }));
      resolve(out);
    };
    tx.onerror = () => reject(tx.error ?? new Error('IndexedDB transaction failed'));
  });
}

export async function clearStoryCache(storyId: string): Promise<void> {
  const chapters = await listCachedChapters(storyId);
  await Promise.all(chapters.map(c => removeCachedChapter(storyId, c.path)));
}

export interface ChapterWordStats {
  chapterCount: number;
  totalWords: number;
  wordCounts: number[];
}

export async function getChapterWordStats(storyId: string): Promise<ChapterWordStats> {
  const chapters = await listCachedChapters(storyId);
  const wordCounts = chapters.map(c => c.wordCount);
  return {
    chapterCount: chapters.length,
    totalWords: wordCounts.reduce((a, b) => a + b, 0),
    wordCounts,
  };
}
