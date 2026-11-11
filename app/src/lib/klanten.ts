import { invoke } from '@tauri-apps/api/core';

// De klantnamen die Mondig onthoudt als suggestie bij het veld Klant (UI-E): de commando's uit
// app/src-tauri/src/klanten.rs en de pure hulpjes voor de suggestielijst. De namen blijven op deze
// computer; een naam komt erbij na een geslaagd opslaan, nooit bij het typen.

/** Zoveel suggesties staan hooguit tegelijk in beeld. */
export const MAX_SUGGESTIES = 6;

/** De klantnamen, nieuwste eerst. */
export function klantenLijst(): Promise<string[]> {
  return invoke<string[]>('klanten_lijst');
}

/** Vergeet alle klantnamen. Opgeslagen bestanden en verslagen blijven zoals ze zijn. */
export function klantenVerwijder(): Promise<void> {
  return invoke<undefined>('klanten_verwijder');
}

/** Een suggestie, in drie stukken zodat het gevonden stuk kan worden gemarkeerd. */
export interface Suggestie {
  naam: string;
  voor: string;
  /** Het stuk dat bij wat er getypt is hoort; leeg als er niets getypt is. */
  gevonden: string;
  na: string;
}

/**
 * De suggesties bij wat er getypt is: namen die de tekst bevatten, zonder verschil in
 * hoofdletters, met namen die ermee beginnen eerst (de volgorde blijft verder die van de lijst:
 * nieuwste eerst). Een naam die precies gelijk is aan wat er staat, is geen suggestie meer. Zonder
 * tekst zijn het de nieuwste namen. Hooguit `max`.
 */
export function filterKlanten(
  lijst: readonly string[],
  tekst: string,
  max: number = MAX_SUGGESTIES
): Suggestie[] {
  const zoek = tekst.trim();
  if (zoek === '') {
    return lijst.slice(0, max).map((naam) => ({ naam, voor: '', gevonden: '', na: naam }));
  }
  const klein = zoek.toLowerCase();
  const treffers: { suggestie: Suggestie; begin: number }[] = [];
  for (const naam of lijst) {
    if (naam === zoek) continue;
    const laag = naam.toLowerCase();
    const begin = laag.indexOf(klein);
    if (begin === -1) continue;
    // Een hoofdletter die bij het kleinmaken van lengte verandert, zou de stukken verschuiven.
    const veilig = laag.length === naam.length;
    treffers.push({
      begin,
      suggestie: veilig
        ? {
            naam,
            voor: naam.slice(0, begin),
            gevonden: naam.slice(begin, begin + klein.length),
            na: naam.slice(begin + klein.length)
          }
        : { naam, voor: naam, gevonden: '', na: '' }
    });
  }
  const aan = treffers.filter((t) => t.begin === 0);
  const rest = treffers.filter((t) => t.begin !== 0);
  return [...aan, ...rest].slice(0, max).map((t) => t.suggestie);
}

/**
 * Welke suggestie actief wordt na een pijltoets: omlaag of omhoog, en rond aan de randen. Zonder
 * actieve suggestie (-1) gaat omlaag naar de eerste en omhoog naar de laatste. Zonder
 * suggesties blijft het -1.
 */
export function volgendeActief(actief: number, richting: 1 | -1, aantal: number): number {
  if (aantal <= 0) return -1;
  if (actief < 0) return richting === 1 ? 0 : aantal - 1;
  return (actief + richting + aantal) % aantal;
}

/** Waar de suggestielijst van het veld Klant staat: open of dicht, en welke suggestie actief is. */
export interface ComboStand {
  open: boolean;
  /** De suggestie die de pijltjes aanwijzen; -1: geen, dan geldt de getypte tekst. */
  actief: number;
}

/** Wat een toets in het veld Klant doet. */
export interface ComboUitkomst {
  /** De nieuwe stand van de lijst. */
  stand: ComboStand;
  /** De suggestie die Enter overneemt, of `null`. */
  kies: number | null;
  /** Enter zonder gekozen suggestie: bewaar wat er staat. */
  klaar: boolean;
  /** De toets niet laten werken in het invoerveld (de cursor springt anders). */
  blokkeer: boolean;
  /** De toets niet doorgeven aan wat eromheen open staat (Escape sluit alleen de lijst). */
  stopVerder: boolean;
}

const DICHT: ComboStand = { open: false, actief: -1 };

/**
 * De toetsen van het patroon "combobox with list autocomplete" (WAI-ARIA APG): pijltjes kiezen
 * (rond aan de randen, en de lijst opent als hij dicht was), Enter neemt de gekozen suggestie over
 * of, zonder gekozen suggestie, bewaart wat er staat (vrije tekst mag), Escape sluit alleen de
 * lijst, Tab sluit hem. Er wordt nooit vanzelf een suggestie gekozen.
 */
export function comboToets(stand: ComboStand, toets: string, aantal: number): ComboUitkomst {
  const gelijk: ComboUitkomst = {
    stand,
    kies: null,
    klaar: false,
    blokkeer: false,
    stopVerder: false
  };
  switch (toets) {
    case 'ArrowDown':
    case 'ArrowUp': {
      if (aantal === 0) return gelijk;
      const richting = toets === 'ArrowDown' ? 1 : -1;
      const actief = volgendeActief(stand.open ? stand.actief : -1, richting, aantal);
      return { ...gelijk, blokkeer: true, stand: { open: true, actief } };
    }
    case 'Enter':
      if (stand.open && aantal > 0 && stand.actief >= 0) {
        return { ...gelijk, blokkeer: true, kies: stand.actief, stand: DICHT };
      }
      return { ...gelijk, klaar: true, stand: DICHT };
    case 'Escape':
      return stand.open ? { ...gelijk, blokkeer: true, stopVerder: true, stand: DICHT } : gelijk;
    case 'Tab':
      return { ...gelijk, stand: DICHT };
    default:
      return gelijk;
  }
}
