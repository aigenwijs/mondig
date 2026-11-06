import { invoke } from '@tauri-apps/api/core';
import type { MapInfo } from './bewaren';
import type { Berichten } from './i18n/typen';

// De bewaakte map (fase B-1): de commando's uit app/src-tauri/src/bewaakte_map.rs. De interface
// geeft nooit een pad door: de map komt uit de mapkiezer, die Rust opent.

/** Waarom de bewaking niet werkt, als de gebruiker hem wel aanzette. */
export type BewaakProbleem = 'geenMap' | 'mapOntbreekt' | 'zelfdeAlsOpslag' | 'teVeel';

/** De keuzes van de bewaakte map. */
export interface BewaakInstellingen {
  aan: boolean;
  map: MapInfo | null;
  submappen: boolean;
  /** Overslaan als er al een transcript is (standaard aan). */
  overslaan: boolean;
  /** Het transcript naast het bronbestand zetten in plaats van in de standaardmap. */
  naast: boolean;
  probleem: BewaakProbleem | null;
  /** Waarom het laatste bestand niet in de wachtrij kon (geen model), of `null`. */
  melding: string | null;
}

export function bewaakInstellingen(): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_instellingen');
}

/** Opent de mapkiezer (in Rust) en bewaart de gekozen map. */
export function bewaakKiesMap(): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_kies_map');
}

export function bewaakZetAan(aan: boolean): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_zet_aan', { aan });
}

export function bewaakZetSubmappen(aan: boolean): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_zet_submappen', { aan });
}

export function bewaakZetOverslaan(aan: boolean): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_zet_overslaan', { aan });
}

export function bewaakZetNaast(aan: boolean): Promise<BewaakInstellingen> {
  return invoke<BewaakInstellingen>('bewaak_zet_naast', { aan });
}

/** De tekst bij een probleem, in gewone taal; `null` als er geen probleem is. */
export function probleemTekst(probleem: BewaakProbleem | null, t: Berichten): string | null {
  switch (probleem) {
    case 'geenMap':
      return t.bewaakKiezenEerst;
    case 'mapOntbreekt':
      return t.bewaakProbleemMapOntbreekt;
    case 'zelfdeAlsOpslag':
      return t.bewaakProbleemZelfdeAlsOpslag;
    case 'teVeel':
      return t.bewaakProbleemTeVeel;
    case null:
      return null;
  }
}
