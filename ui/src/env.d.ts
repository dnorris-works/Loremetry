/// <reference types="vite/client" />
/// <reference types="@tauri-apps/api" />

interface ImportMetaEnv {
  readonly TAURI_ENV_PLATFORM?: string;
  readonly TAURI_ENV_ARCH?: string;
  readonly TAURI_ENV_FAMILY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
