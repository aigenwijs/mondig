import { invoke } from '@tauri-apps/api/core';
import type { LaatsteControle } from './computer';
import type { Modelrol } from './vergadering';

// De pagina Modellen (sprint 5-A): de commando's van Rust en de kleine regels erachter. De
// echte regels (welk model waarvoor te kiezen is, wat niet weg mag) staan in Rust; wat hier
// staat bepaalt alleen hoe de pagina het toont. Weigert een commando, dan is de afwijzing een
// `Foutmelding` (zie `foutUitCommando` in taak.ts).

/** De twee taken waarvoor je een model kiest. Dezelfde namen als `Gebruik` in Rust. */
export type Gebruik = 'dicteren' | 'bestanden';

export const GEBRUIKEN: readonly Gebruik[] = ['dicteren', 'bestanden'];

/** Waar een model voor dient. Dezelfde namen als `Soort` in Rust. */
export type Soort = 'spraak' | 'sprekers' | 'privacy' | 'hulp';

/** Een stap op een balkje: 1 tot 5. */
export type Stap = 1 | 2 | 3 | 4 | 5;

/** Hoe ver een model op schijf is. */
export type ModelStand = 'ontbreekt' | 'gedeeltelijk' | 'compleet';

/** Eén model op de pagina, zoals Rust het teruggeeft. */
export interface ModelRegel {
  id: string;
  /** De naam zonder de uitleg tussen haakjes, bijvoorbeeld "Canary 1b-v2". */
  naam: string;
  /** Waar het model voor dient; de pagina deelt hierop in. */
  soort: Soort;
  /**
   * De rol (snel, nauwkeurig, extra nauwkeurig) waarmee het model buiten de beheerlijst heet;
   * `null` voor een model dat niet voor opnames en bestanden te kiezen is (het sprekermodel).
   */
  rol: Modelrol | null;
  /** Nauwkeurigheid in vijf vaste stappen, afgeleid van onze bench (zie `modellen.json`). */
  nauwkeurigheid: Stap;
  /** Snelheid in vijf vaste stappen, afgeleid van onze bench (zie `modellen.json`). */
  snelheid: Stap;
  /** Ondersteunde talen als ISO 639-codes. */
  talen: string[];
  status: ModelStand;
  /** Wat er in totaal te downloaden is. */
  grootteBytes: number;
  /** Wat er al op schijf staat, ook van een afgebroken download. */
  aanwezigBytes: number;
  /** Hoeveel ruimte er te weinig is om te downloaden (met de marge); 0 als het past. */
  tekortBytes: number;
  /** Taken waarvoor het model niet te kiezen is. */
  nietVoor: Gebruik[];
  /** Voor welke taken dit model nu in gebruik is. */
  inGebruikVoor: Gebruik[];
  /** Mag het model nu van de computer? */
  kanWeg: boolean;
  /** Zag de gebruiker dit model nog niet staan? */
  nieuw: boolean;
  bron: string;
  /**
   * Voor welke taken het model in gebruik komt als "Verder downloaden" afmaakt wat "Downloaden
   * en gebruiken" begon. Leeg bij een gewone download: hervatten verandert dan niets.
   */
  daarnaBijHervatten: Gebruik[];
}

/** Antwoord van `modellen_overzicht`. */
export interface ModellenOverzicht {
  modellen: ModelRegel[];
  /** Het model dat voor dicteren geldt. */
  dicteren: string;
  /** Het model dat voor bestanden geldt. */
  bestanden: string;
  /** Het model dat nu binnenkomt, als er een download loopt. */
  downloadt: string | null;
  /** Wat alle modellen samen op de schijf innemen. */
  totaalBytes: number;
  vrijBytes: number | null;
}

/** Het overzicht voor de pagina en de zijbalk. */
export function modellenOverzicht(): Promise<ModellenOverzicht> {
  return invoke<ModellenOverzicht>('modellen_overzicht');
}

/** De gebruiker heeft de pagina geopend: wat er staat, is niet meer nieuw. */
export function modellenGezien(): Promise<void> {
  return invoke<undefined>('modellen_gezien');
}

/** Zet een model dat al op de computer staat in gebruik voor een taak. */
export function modelGebruikZet(gebruik: Gebruik, model: string): Promise<void> {
  return invoke<undefined>('model_gebruik_zet', { gebruik, model });
}

/**
 * Kiest een model voor één taak (Wijzigen op Modellen): meteen als het er staat, anders na de
 * download. Een andere download verliest deze taak.
 */
export function modelKiezen(gebruik: Gebruik, model: string): Promise<unknown> {
  return invoke<unknown>('model_kiezen', { gebruik, model });
}

/** Haalt een model van de computer. Geeft het aantal vrijgemaakte bytes. */
export function modelVerwijder(model: string): Promise<number> {
  return invoke<number>('model_verwijder', { model });
}

/**
 * Start de download van een model. Na een geslaagde download krijgt het de taken in `daarna`.
 * Met `null` (verder downloaden) gelden de taken die Rust bij de eerste start onthield.
 */
export function modelDownloadStart(
  model: string,
  daarna: readonly Gebruik[] | null
): Promise<number> {
  return invoke<number>('model_download_start', { model, daarna });
}

/**
 * De soorten die Modellen toont, in deze volgorde. Een hulpmodel (Silero) is nooit zichtbaar.
 */
export const SOORTEN: readonly Exclude<Soort, 'hulp'>[] = ['spraak', 'sprekers', 'privacy'];

/** Hoeveel talen een model kent, en kent het ook Turks en Arabisch? */
export function talenSamenvatting(regel: ModelRegel): { aantal: number; turksArabisch: boolean } {
  return {
    aantal: regel.talen.length,
    turksArabisch: regel.talen.includes('tr') && regel.talen.includes('ar')
  };
}

// ---------------------------------------------------------------------------
// Wat een rij kan
// ---------------------------------------------------------------------------

/** Is het model voor deze taak te kiezen? */
export function kiesbaarVoor(regel: ModelRegel, gebruik: Gebruik): boolean {
  return !regel.nietVoor.includes(gebruik);
}

/**
 * Wat de downloadknop in de beheerkaart zegt: een gewone download, of "Verder downloaden" (dan
 * maakt Rust af wat bij de eerste start gekozen werd).
 */
export type DownloadKnop = 'gewoon' | 'verder';

/** Wat een rij nu toont en kan, afgeleid van het model en van de downloadrij. */
export interface RijStand {
  /** Dit model staat in de downloadrij: het wacht of komt nu binnen. */
  bezig: boolean;
  /** De knop om te downloaden, of `null` als er niets te downloaden valt. */
  download: DownloadKnop | null;
  /** Past het model niet op de schijf? */
  teWeinigRuimte: boolean;
  /** Is "Verwijderen" er (aan of uit)? */
  verwijderen: 'aan' | 'uit' | null;
}

/**
 * Bepaalt wat een rij toont. `inRij` zegt of dit model in de downloadrij staat (wacht of
 * binnenkomt). Een download van een ander model houdt deze niet tegen: hij komt erachter.
 */
export function rijStand(regel: ModelRegel, inRij: boolean): RijStand {
  const bezig = inRij;
  const compleet = regel.status === 'compleet';
  const teWeinigRuimte = !compleet && !bezig && regel.tekortBytes > 0;
  // Bij te weinig ruimte blijft de knop staan, maar uit: zo ziet de gebruiker wat er zou kunnen.
  let download: DownloadKnop | null = null;
  if (!compleet && !bezig) {
    download = regel.status === 'gedeeltelijk' ? 'verder' : 'gewoon';
  }
  let verwijderen: RijStand['verwijderen'] = null;
  if (regel.status !== 'ontbreekt' && !bezig) verwijderen = regel.kanWeg ? 'aan' : 'uit';
  return {
    bezig,
    download,
    teWeinigRuimte,
    verwijderen
  };
}

/** Het model met dit id. */
export function zoekModel(modellen: readonly ModelRegel[], id: string): ModelRegel | undefined {
  return modellen.find((m) => m.id === id);
}

// ---------------------------------------------------------------------------
// Indeling na de gebruikerstest van N-C: functiekaarten bovenaan, beheer eronder
// ---------------------------------------------------------------------------

/**
 * De spraakmodellen die bij een functie passen: alleen wat voor die taak te kiezen is. Een model
 * dat te traag is om te dicteren, komt bij Dicteren dus niet voor.
 */
export function passendVoor(modellen: readonly ModelRegel[], gebruik: Gebruik): ModelRegel[] {
  return modellen.filter((m) => m.soort === 'spraak' && kiesbaarVoor(m, gebruik));
}

/** De beheerlijst: eerst wat op de computer staat, dan wat te downloaden is. */
export interface Beheer {
  opDezeComputer: ModelRegel[];
  teDownloaden: ModelRegel[];
}

/**
 * Deelt de modellen in voor de beheerlijst, spraakmodellen eerst en het sprekermodel erachter.
 * Een download die half af is of nog loopt, staat bij "Te downloaden", zodat een kaart niet
 * verspringt terwijl hij binnenkomt.
 */
export function beheerIndeling(modellen: readonly ModelRegel[]): Beheer {
  const zichtbaar = SOORTEN.flatMap((soort) => modellen.filter((m) => m.soort === soort));
  return {
    opDezeComputer: zichtbaar.filter((m) => m.status === 'compleet'),
    teDownloaden: zichtbaar.filter((m) => m.status !== 'compleet')
  };
}

/**
 * Raadt de laatste controle dit model aan voor deze functie? Dan krijgt het "Past bij deze
 * computer". Zonder controle nooit: Mondig zegt alleen wat het gecontroleerd heeft.
 */
export function pastBijComputer(
  controle: LaatsteControle | null,
  gebruik: Gebruik,
  model: string
): boolean {
  if (controle === null) return false;
  return (gebruik === 'dicteren' ? controle.dicteren : controle.bestanden) === model;
}
