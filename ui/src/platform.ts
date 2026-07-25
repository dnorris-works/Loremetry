/** True when running inside the Tauri desktop shell (Vite sets TAURI_ENV_*). */
export function isDesktopApp(): boolean {
  return import.meta.env.TAURI_ENV_PLATFORM !== undefined;
}
