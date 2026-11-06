import { invoke, isTauri } from '@tauri-apps/api/core';

/** Hoe ver het spraakmodel op schijf is. */
export type ModelStatus = 'ontbreekt' | 'gedeeltelijk' | 'compleet';

/** Antwoord van het Tauri-commando `model_status`. */
export interface ModelOverzicht {
  id: string;
  status: ModelStatus;
  grootteBytes: number;
  aanwezigBytes: number;
  /** Vrije schijfruimte, of `null` als die niet te meten is. */
  vrijBytes: number | null;
  /** Hoeveel ruimte er te weinig is om te downloaden (met marge); 0 als er genoeg is. */
  tekortBytes: number;
  /** Host waar het model vandaan komt, bijvoorbeeld `huggingface.co`. */
  bron: string;
}

// Terugval voor de browser-weergave (buiten Tauri), zodat de schermen na te lopen zijn.
// In de app komt de echte grootte altijd uit de catalogus.
const ZONDER_APP: ModelOverzicht = {
  id: 'parakeet-ultra-q8',
  status: 'ontbreekt',
  grootteBytes: 740_000_000,
  aanwezigBytes: 0,
  vrijBytes: null,
  tekortBytes: 0,
  bron: 'huggingface.co'
};

/**
 * Vraagt de app naar de status van het standaardmodel. Buiten Tauri bestaat het commando
 * niet en geven we "ontbreekt" terug. In de app laten we een fout door: de aanroeper toont
 * die in gewone taal, in plaats van te doen alsof het model ontbreekt.
 */
export async function haalModelStatus(): Promise<ModelOverzicht> {
  if (!isTauri()) return ZONDER_APP;
  return await invoke<ModelOverzicht>('model_status');
}
