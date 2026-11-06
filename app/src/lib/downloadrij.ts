import type { DownloadVoortgang, Vervolg } from './taak';

// De wachtrij met modeldownloads (Rust: `mondig_core::downloadrij`, commando's in
// `downloads.rs`). Hier staan de typen en de beslissingen zonder scherm: wat een knop zegt, wat
// de zijbalk toont. De stand zelf houdt `modellenstand.svelte.ts` bij.

/** Waar een download in de rij staat. De vorm volgt de serde-uitvoer van Rust. */
export type RijToestand =
  | { type: 'wacht' }
  | { type: 'bezig' }
  | { type: 'nieuwe_poging'; poging: number; van: number }
  | { type: 'klaar' }
  | { type: 'fout'; melding: string; vervolg: Vervolg }
  | { type: 'gestopt' }
  | { type: 'onderbroken' };

/** Eén download in de rij (event `downloads-gewijzigd`, commando `downloads_lijst`). */
export interface DownloadItem {
  id: number;
  model: string;
  toestand: RijToestand;
  voortgang: DownloadVoortgang | null;
}

/**
 * De hele rij met een volgnummer (event `downloads-gewijzigd`, commando `downloads_lijst`). Het
 * volgnummer loopt op bij elke wijziging; een oudere stand die later binnenkomt, telt niet.
 */
export interface RijStand {
  versie: number;
  items: DownloadItem[];
}

/** Wacht de download nog of loopt hij? */
export function isActief(toestand: RijToestand): boolean {
  return (
    toestand.type === 'wacht' || toestand.type === 'bezig' || toestand.type === 'nieuwe_poging'
  );
}

/** Komt de download nu binnen (ook tijdens een nieuwe poging)? */
export function isBezig(toestand: RijToestand): boolean {
  return toestand.type === 'bezig' || toestand.type === 'nieuwe_poging';
}

/** Voortgang in hele procenten, 0 als die nog niet bekend is. */
export function procentVan(voortgang: DownloadVoortgang | null | undefined): number {
  if (!voortgang || voortgang.totaal <= 0) return 0;
  return Math.min(100, Math.floor((voortgang.binnen * 100) / voortgang.totaal));
}

/** Het item van een model. Rust houdt er per model hooguit één bij. */
export function itemVoor(rij: readonly DownloadItem[], model: string): DownloadItem | undefined {
  return rij.find((item) => item.model === model);
}

/**
 * Wat een Downloaden-knop laat zien:
 * - `start`: de gewone knop;
 * - `wordt_gestart`: net geklikt, Rust heeft nog niet geantwoord ("Wordt gestart…", uit);
 * - `wacht`: staat in de rij achter een andere download;
 * - `bezig`: komt binnen, met voortgang;
 * - `opnieuw`: de verbinding viel weg en Mondig probeert het zelf opnieuw.
 */
export type KnopFase = 'start' | 'wordt_gestart' | 'wacht' | 'bezig' | 'opnieuw';

/**
 * De fase van de knop. Een actief item in de rij wint, zodat de knop meteen doorschuift naar
 * "wacht" of de voortgang zodra Rust het meldt, ook als het antwoord op de klik nog onderweg is.
 */
export function knopFase(startend: boolean, item: DownloadItem | undefined): KnopFase {
  switch (item?.toestand.type) {
    case 'wacht':
      return 'wacht';
    case 'bezig':
      return 'bezig';
    case 'nieuwe_poging':
      return 'opnieuw';
    default:
      return startend ? 'wordt_gestart' : 'start';
  }
}

/**
 * Verdwijnt het item na korte tijd vanzelf uit de zijbalk? Ja als het klaar is of door de
 * gebruiker zelf gestopt. Een fout of een onderbroken download blijft staan tot de gebruiker
 * de pagina Modellen gezien heeft.
 */
export function verdwijntVanzelf(toestand: RijToestand): boolean {
  return toestand.type === 'klaar' || toestand.type === 'gestopt';
}

/** Zo lang blijft een afgeronde download nog in de zijbalk staan, in milliseconden. */
export const ZIJBALK_NAGLOEI_MS = 4000;

/**
 * Welke items de zijbalk toont: alles wat wacht of loopt, en wat afgelopen is zolang het niet
 * verborgen is (zie `verdwijntVanzelf`).
 */
export function zijbalkItems(
  rij: readonly DownloadItem[],
  verborgen: ReadonlySet<number>
): DownloadItem[] {
  return rij.filter((item) => isActief(item.toestand) || !verborgen.has(item.id));
}
