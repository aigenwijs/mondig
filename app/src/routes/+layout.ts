import { isTauri } from '@tauri-apps/api/core';

// Tauri serveert een statische app: geen server-side rendering, geen prerender.
export const ssr = false;

export async function load(): Promise<void> {
  // Alleen in `vite dev` buiten de app: doe alsof de app er is, zodat de schermen na te lopen
  // zijn. Staat achter DEV, dus de productiebuild neemt de mock niet mee.
  if (import.meta.env.DEV && !isTauri()) {
    const { installeerDevMock } = await import('#lib/dev/mock.js');
    installeerDevMock();
  }
}
