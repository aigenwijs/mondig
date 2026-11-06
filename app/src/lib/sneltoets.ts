import { invoke, isTauri } from '@tauri-apps/api/core';

/** Antwoord van het Tauri-commando `sneltoets_status`. */
export interface SneltoetsStatus {
  /** De actieve toets in gewone taal (`Ctrl+Spatie`), of `null` als geen enkele lukte. */
  actief: string | null;
  /** Is de actieve toets een terugvaloptie omdat de gewenste niet vrij was? */
  isTerugval: boolean;
  /** De gewenste toets in gewone taal. */
  gewenst: string;
  /** Alle toetsen die geprobeerd zijn, in gewone taal. */
  geprobeerd: string[];
  /** Alleen bij de tweede toets: staat hij uit omdat hij gelijk is aan de gewone? */
  dubbel?: boolean;
}

/**
 * Welke sneltoets: de gewone dicteertoets, of de tweede die een opgeschoond dictaat geeft
 * (ADR 0027). Zonder: de gewone.
 */
export type SneltoetsSoort = 'dicteren' | 'opschonen';

/** De stand van de sneltoets. Buiten de app (gewone browser) bestaat hij niet: `null`. */
export async function haalSneltoetsStatus(
  soort: SneltoetsSoort = 'dicteren'
): Promise<SneltoetsStatus | null> {
  if (!isTauri()) return null;
  return await invoke<SneltoetsStatus>('sneltoets_status', { soort });
}

/**
 * Schort beide globale sneltoetsen op, zodat Ctrl+Spatie tijdens het vangen geen dictaat start.
 * Weigert als de toets niet betrouwbaar af te melden is; dan mag het vangen niet beginnen.
 */
export function sneltoetsVangStart(): Promise<void> {
  return invoke<undefined>('sneltoets_vang_start');
}

/** Zet de opgeschorte sneltoetsen terug en geeft de stand. Mag altijd, ook zonder opschorten. */
export function sneltoetsVangStop(soort: SneltoetsSoort = 'dicteren'): Promise<SneltoetsStatus> {
  return invoke<SneltoetsStatus>('sneltoets_vang_stop', { soort });
}

/**
 * Registreert een nieuwe sneltoets (schrijfwijze van de plugin, `Ctrl+Alt+Space`). Lukt het
 * niet, dan staat de oude toets er weer en weigert dit met `vervolg: 'opnieuw_proberen'`.
 */
export function sneltoetsWijzig(
  toets: string,
  soort: SneltoetsSoort = 'dicteren'
): Promise<SneltoetsStatus> {
  return invoke<SneltoetsStatus>('sneltoets_wijzig', { toets, soort });
}

/** Waarom een sneltoets niet kan (Rust: `SneltoetsFout`). */
export type SneltoetsFoutSoort =
  | 'leeg'
  | 'windowsToets'
  | 'losseToets'
  | 'shiftAlleen'
  | 'geenHoofdtoets'
  | 'meerdereHoofdtoetsen'
  | 'escNietToegestaan'
  | 'gereserveerd'
  | 'onbekend'
  | 'werktNiet';

/** Het oordeel van Rust over een sneltoets (Rust: `Oordeel`). */
export type Oordeel =
  | { uitkomst: 'goed' }
  /** Mag na bevestiging. `toets` is de gewone toets, bijvoorbeeld `F5`. */
  | { uitkomst: 'waarschuwing'; soort: 'functietoets'; toets: string }
  | { uitkomst: 'fout'; fout: SneltoetsFoutSoort };

/**
 * Waarom `sneltoets_wijzig` weigert: de toets mag niet, een ander programma heeft hem al, of het
 * is al de andere sneltoets van Mondig.
 */
export type WijzigFout =
  { soort: 'geweigerd'; fout: SneltoetsFoutSoort } | { soort: 'bezet' } | { soort: 'dubbel' };

/** Herkent de fout van `sneltoets_wijzig`; `null` bij elke andere fout. */
export function alsWijzigFout(fout: unknown): WijzigFout | null {
  if (typeof fout !== 'object' || fout === null || !('soort' in fout)) return null;
  if (fout.soort === 'bezet') return { soort: 'bezet' };
  if (fout.soort === 'dubbel') return { soort: 'dubbel' };
  if (fout.soort === 'geweigerd' && 'fout' in fout && typeof fout.fout === 'string') {
    return { soort: 'geweigerd', fout: fout.fout as SneltoetsFoutSoort };
  }
  return null;
}

/**
 * Vraagt Rust of een sneltoets (schrijfwijze van de plugin, `Ctrl+Alt+Space`) mag. De regels
 * staan op een plek, in `mondig-core`; dit is alleen de brug.
 */
export function sneltoetsControleer(toets: string): Promise<Oordeel> {
  return invoke<Oordeel>('sneltoets_controleer', { toets });
}

/**
 * Houdt bij welk antwoord het laatste is. Elke aanvraag die een resultaat in de scherm-
 * toestand zet, vraagt eerst `nieuw()`; komt het antwoord later binnen, dan zet het alleen iets
 * als `isNogHuidig` nog klopt. Zo overschrijft een traag antwoord geen nieuwere toestand.
 */
export function maakVolgorde(): { nieuw: () => () => boolean; ongeldig: () => void } {
  let teller = 0;
  return {
    nieuw: () => {
      teller += 1;
      const mijn = teller;
      return () => mijn === teller;
    },
    ongeldig: () => {
      teller += 1;
    }
  };
}
