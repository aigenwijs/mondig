// De dictaatgeschiedenis (ADR 0021): de vormen die Rust stuurt en wat de interface ermee doet
// zonder Tauri, zodat het te testen is. De bestanden, de termijn en het wissen zitten in Rust.

import type { OpgeschoondDoor } from './opschonen';

/** Hoe lang Mondig dictaten bewaart: de waarden van de store (`dictaten_bewaren`). */
export type DictatenTermijn = 'niet' | '24u' | '7d';

/** De drie keuzes, in de volgorde van de keuzelijst. */
export const DICTATEN_TERMIJNEN: readonly DictatenTermijn[] = ['niet', '24u', '7d'];

/** De standaard (ADR 0021 punt 5), ook als Rust niet te bereiken is. */
export const STANDAARD_DICTATEN_TERMIJN: DictatenTermijn = '24u';

/** Een bewaard dictaat zoals Rust het stuurt. */
export interface BewaardDictaat {
  /** De bestandsnaam; alleen om het dictaat terug te geven bij verwijderen. */
  id: string;
  /** Het moment waarop de tekst klaar was (ms sinds 1970). */
  tijdUnixMs: number;
  /** De tekst zoals hij geplakt is. */
  tekst: string;
  /** Bij een opgeschoond dictaat (ADR 0027): de letterlijke tekst. */
  letterlijk?: string;
  /** Bij een opgeschoond dictaat: welk model bij welke aanbieder het opschoonde. */
  opgeschoondDoor?: OpgeschoondDoor;
}

/** Het laatste dictaat in het geheugen van de app. */
export interface LaatsteDictaat {
  tekst: string;
  /** Het id van het bestand; `null` als het niet op schijf staat (Niet bewaren, of schrijven mislukte). */
  id: string | null;
}

/** Het antwoord van `dictaten_stand`: de termijn en de lijst, nieuwste eerst. */
export interface DictatenAntwoord {
  termijn: DictatenTermijn;
  dictaten: BewaardDictaat[];
  /** Het aantal bestanden op schijf, ook wat nog moet verlopen: zoveel gaat weg bij Wissen. */
  bewaard: number;
  laatste: LaatsteDictaat | null;
}

const UUR_MS = 3_600_000;
const DUUR_MS: Record<DictatenTermijn, number> = {
  niet: 0,
  '24u': 24 * UUR_MS,
  '7d': 7 * 24 * UUR_MS
};

/**
 * Laat weg wat verlopen is, met dezelfde regel als Rust (`tijd + termijn <= nu`, of meer dan
 * een uur in de toekomst). Zo toont de lijst nooit iets ouders dan de termijn, ook tussen twee
 * opruimrondes van Rust in (ADR 0021 punt 4).
 */
export function filterVerlopen(
  dictaten: readonly BewaardDictaat[],
  termijn: DictatenTermijn,
  nu: number
): BewaardDictaat[] {
  return dictaten.filter(
    (d) => d.tijdUnixMs + DUUR_MS[termijn] > nu && d.tijdUnixMs <= nu + UUR_MS
  );
}

/**
 * Staat het laatste dictaat niet als nieuwste in de lijst (het schrijven naar schijf mislukte,
 * of het is net verlopen)? Dan toont Dicteren het los, zodat de tekst nooit nergens staat.
 * Vergelijkt op id, niet op tekst: twee dictaten kunnen dezelfde tekst hebben.
 */
export function laatsteLosTonen(
  laatste: LaatsteDictaat | null,
  zichtbaar: readonly BewaardDictaat[]
): boolean {
  if (laatste === null) return false;
  return laatste.id === null || laatste.id !== zichtbaar[0]?.id;
}

/** Is dit een termijn die Rust kent? Een onbekende waarde valt terug op de standaard. */
export function termijnUit(waarde: unknown): DictatenTermijn {
  return DICTATEN_TERMIJNEN.find((t) => t === waarde) ?? STANDAARD_DICTATEN_TERMIJN;
}

/** Een groep dictaten van één dag. */
export interface DagGroep {
  /** De lokale datum, `2026-11-04`; ook de sleutel voor de lijst. */
  sleutel: string;
  soort: 'vandaag' | 'gisteren' | 'datum';
  /** Een moment op die dag, voor de opmaak van de datum. */
  tijdUnixMs: number;
  dictaten: BewaardDictaat[];
}

/** De lokale dag van een moment als `jjjj-mm-dd`. */
function dagSleutel(ms: number): string {
  const d = new Date(ms);
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${String(d.getFullYear())}-${mm}-${dd}`;
}

/**
 * Deelt dictaten (nieuwste eerst) in per lokale dag, in dezelfde volgorde. "Vandaag" en
 * "Gisteren" gaan op de kalenderdag van `nu`, niet op 24 uur terug.
 */
export function groepeerPerDag(dictaten: readonly BewaardDictaat[], nu: number): DagGroep[] {
  const vandaag = dagSleutel(nu);
  const gisterenDatum = new Date(nu);
  gisterenDatum.setDate(gisterenDatum.getDate() - 1);
  const gisteren = dagSleutel(gisterenDatum.getTime());
  const groepen: DagGroep[] = [];
  for (const dictaat of dictaten) {
    const sleutel = dagSleutel(dictaat.tijdUnixMs);
    const laatste = groepen.at(-1);
    if (laatste?.sleutel === sleutel) {
      laatste.dictaten.push(dictaat);
      continue;
    }
    groepen.push({
      sleutel,
      soort: sleutel === vandaag ? 'vandaag' : sleutel === gisteren ? 'gisteren' : 'datum',
      tijdUnixMs: dictaat.tijdUnixMs,
      dictaten: [dictaat]
    });
  }
  return groepen;
}

/** De datum als kop boven een groep, bijvoorbeeld "woensdag 28 oktober". */
export function datumKop(ms: number, taal: string): string {
  return new Intl.DateTimeFormat(taal, { weekday: 'long', day: 'numeric', month: 'long' }).format(
    new Date(ms)
  );
}

/**
 * Moet Mondig eerst vragen voor deze wijziging van de termijn? Alleen bij Niet bewaren, en
 * alleen als er bewaarde dictaten zijn die daarmee wegaan (ADR 0021 punt 6).
 */
export function vraagVoorTermijn(nieuw: DictatenTermijn, bewaardAantal: number): boolean {
  return nieuw === 'niet' && bewaardAantal > 0;
}
