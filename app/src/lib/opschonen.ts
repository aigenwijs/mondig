import { invoke } from '@tauri-apps/api/core';
import type { NotulenAanbieder } from './notulen';

// Dictaten opschonen (ADR 0027): de commando's uit app/src-tauri/src/opschonen.rs en de pure
// hulpjes voor Instellingen, de pil en de geschiedenis. Rust beslist of opschonen aan mag (een
// aanbieder onder Notulen, en per aanbieder één keer de vraag waar de tekst heen gaat); de
// interface toont en vraagt alleen.

/** Wanneer Mondig opschoont (Rust: `Stand`). */
export type OpschoonStand = 'uit' | 'eigenSneltoets' | 'bijElkDictaat';

/** De keuzes in de volgorde van de keuzelijst; Uit is de standaard. */
export const OPSCHOON_STANDEN: readonly OpschoonStand[] = [
  'uit',
  'eigenSneltoets',
  'bijElkDictaat'
];

/** Het label van een aanbieder, zoals overal in Mondig. */
export type Label = 'lokaal' | 'europees' | 'buiten_europa';

/** Wat Rust van opschonen geeft. */
export interface OpschonenInfo {
  /** De stand die geldt: uit zonder aanbieder, of zolang de vraag niet beantwoord is. */
  stand: OpschoonStand;
  /** De aanbieder onder Notulen als die klaar is; anders `null`. */
  aanbieder: NotulenAanbieder | null;
  /** De naam van die aanbieder (`Mistral`, `Ollama`), van Rust. */
  naam: string | null;
  /** Het label van die aanbieder, van Rust; de interface leidt het nooit zelf af. */
  label: Label | null;
  /** Het model dat opschoont: bij Mistral altijd Mistral Small, bij Ollama dat van notulen. */
  model: string | null;
  /** Bevestigde de gebruiker al voor deze aanbieder waar de tekst heen gaat? */
  bevestigd: boolean;
}

/** Wat de pil van het opschonen hoort (event `pil-opschonen`). */
export type PilOpschoonFase =
  { type: 'bezig'; label: Label } | { type: 'opgeschoond' } | { type: 'letterlijk' };

export interface PilOpschonen {
  taak: number;
  fase: PilOpschoonFase;
}

/** Wie een bewaard dictaat opschoonde. */
export interface OpgeschoondDoor {
  model: string;
  aanbieder: string;
  /** Het label van de aanbieder, van Rust; ontbreekt in een ouder bestand. */
  label?: Label;
}

// ---------------------------------------------------------------------------
// Commando's
// ---------------------------------------------------------------------------

export function opschonenInstellingen(): Promise<OpschonenInfo> {
  return invoke<OpschonenInfo>('opschonen_instellingen');
}

/**
 * Zet de stand. De eerste keer per aanbieder weigert Rust zonder `bevestigd`: de code van de
 * aanbieder waarvoor de gebruiker de vraag "Waar gaat de tekst heen?" net beantwoordde.
 */
export function opschonenZetStand(
  stand: OpschoonStand,
  bevestigd: NotulenAanbieder | null = null
): Promise<OpschonenInfo> {
  return invoke<OpschonenInfo>('opschonen_zet_stand', { stand, bevestigd });
}

/** Het kruisje in de pil: stoppen met opschonen, de letterlijke tekst wordt geplakt. */
export function dictaatOpschonenStop(taak: number): Promise<void> {
  return invoke<undefined>('dictaat_opschonen_stop', { taak });
}

// ---------------------------------------------------------------------------
// Pure hulpjes
// ---------------------------------------------------------------------------

/** De naam van een model in gewone taal: `mistral-small-latest` wordt Mistral Small. */
export function modelNaam(model: string): string {
  if (model === 'mistral-small-latest') return 'Mistral Small';
  if (model === 'mistral-large-latest') return 'Mistral Large';
  return model;
}

/** Moet de vraag "Waar gaat de tekst heen?" eerst? Bij aanzetten, zolang die niet beantwoord is. */
export function vraagEerst(info: OpschonenInfo, gekozen: OpschoonStand): boolean {
  return gekozen !== 'uit' && info.aanbieder !== null && !info.bevestigd;
}
