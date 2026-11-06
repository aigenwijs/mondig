import { invoke, isTauri } from '@tauri-apps/api/core';

/**
 * Zet tekst op het klembord. In de app gaat dat via Mondig zelf, zodat de tekst buiten de
 * klembordgeschiedenis (Win+V) en de cloud blijft; in de gewone browser (ontwikkeling) via
 * de browser. Geeft `false` als dat niet lukt, zodat de aanroeper dat in gewone taal kan
 * melden. De tekst wordt nergens gelogd.
 */
export async function kopieerNaarKlembord(tekst: string): Promise<boolean> {
  try {
    if (isTauri()) return await invoke<boolean>('klembord_zet', { tekst });
    await navigator.clipboard.writeText(tekst);
    return true;
  } catch {
    return false;
  }
}
