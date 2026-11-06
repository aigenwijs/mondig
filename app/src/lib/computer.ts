import { invoke } from '@tauri-apps/api/core';
import type { Vergelijking, Videokaart } from './videokaart';
import type { Berichten } from './i18n/typen';

// De hardwarecheck (hoofdplan hoofdstuk 9): de commando's van Rust en de kleine regels erachter.
// Het advies zelf wordt in Rust berekend (`mondig_core::hardware`); hier staat alleen hoe het
// getoond wordt. Weigert een commando, dan is de afwijzing een `Foutmelding`.

/** Hoeveel werkgeheugen de computer heeft, in klassen. */
export type Geheugen = 'onbekend' | 'te_weinig' | 'weinig' | 'genoeg' | 'ruim';

/** Wat Mondig over de videokaart zegt. */
export type VideokaartStand = 'geen' | 'ingebouwd' | 'testen' | 'aanzetten' | 'laten' | 'faalde';

/** Wat Mondig van de computer las. `null` is: niet te meten. */
export interface Computer {
  geheugenBytes: number | null;
  kernen: number | null;
  vrijBytes: number | null;
  /** Leeg als er geen videokaart is; `null` als zoeken niet lukte. */
  videokaarten: Videokaart[] | null;
}

/** Het advies voor deze computer. */
export interface Advies {
  geheugen: Geheugen;
  dicteren: string;
  bestanden: string;
  /** Is voor bestanden een tweede model (en dus een tweede download) aan te raden? */
  bestandenApart: boolean;
  /** Een computer van 8 tot 12 GB: twee modellen passen, maar krap. */
  krap: boolean;
  tekortDicterenBytes: number;
  tekortBeideBytes: number;
  videokaart: { stand: VideokaartStand; keerSneller: number | null };
}

/** Antwoord van `computer_check`. */
export interface ComputerInfo {
  computer: Computer;
  advies: Advies;
  dicterenInGebruik: string;
  bestandenInGebruik: string;
}

/** Leest de computer en geeft het advies; met `proef` (de snelheidstest) ook over de videokaart. */
export function computerCheck(proef: Vergelijking | null): Promise<ComputerInfo> {
  return invoke<ComputerInfo>('computer_check', { proef });
}

/**
 * De uitkomst van de laatste controle (Mondig opnieuw afstellen), voor "Past bij deze computer"
 * op Modellen. Alleen de modelkeuzes, de stand van de videokaart en de datum.
 */
export interface LaatsteControle {
  dicteren: string;
  bestanden: string;
  videokaart: VideokaartStand;
  /** Wanneer, in milliseconden sinds 1970. */
  datumMs: number;
}

/** De laatste controle, of `null` als Mondig deze computer nog niet bekeek. */
export function laatsteControle(): Promise<LaatsteControle | null> {
  return invoke<LaatsteControle | null>('laatste_controle');
}

/** Bewaart de uitkomst van een afgeronde controle; Rust zet de datum erbij. */
export function laatsteControleBewaar(advies: Advies): Promise<LaatsteControle> {
  return invoke<LaatsteControle>('laatste_controle_bewaar', {
    dicteren: advies.dicteren,
    bestanden: advies.bestanden,
    videokaart: advies.videokaart.stand
  });
}

/** Eén aangevinkt punt van het advies, zoals `advies_overnemen` het wil. */
export type AdviesInvoer =
  { soort: 'videokaart' } | { soort: 'model'; gebruik: 'dicteren' | 'bestanden'; model: string };

/** Wat `advies_overnemen` deed. */
export interface AdviesUitkomst {
  /** De videokaart staat nu aan. */
  videokaartAan: boolean;
  /** Modellen die meteen in gebruik zijn genomen. */
  gewisseld: { gebruik: 'dicteren' | 'bestanden'; model: string }[];
  /** Modellen die binnenkomen en daarna in gebruik komen. */
  downloadt: { model: string; taken: ('dicteren' | 'bestanden')[] }[];
}

/**
 * Neemt de aangevinkte punten van het advies over: de videokaart gaat aan, een model dat er staat
 * wordt meteen gebruikt, een model dat nog binnen moet komen gaat in de downloadrij en wordt
 * gebruikt zodra het klaar is. Zonder punten verandert er niets.
 */
export function adviesOvernemen(punten: AdviesInvoer[]): Promise<AdviesUitkomst> {
  return invoke<AdviesUitkomst>('advies_overnemen', { punten });
}

/** Het logboek als tekst, met een kop; de aanroeper zet het op het klembord. */
export function logboekTekst(): Promise<string> {
  return invoke<string>('logboek_tekst');
}

/** Opent de logmap in de Verkenner. */
export function logboekMapOpen(): Promise<void> {
  return invoke<undefined>('logboek_map_open');
}

/** Zet alle instellingen terug naar de standaard; de modellen blijven staan. */
export function instellingenTerugzetten(): Promise<{ sneltoetsBezet: boolean }> {
  return invoke<{ sneltoetsBezet: boolean }>('instellingen_terugzetten');
}

/** De maten waarin computers verkocht worden, in GB. */
const GEHEUGENMATEN = [2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128];

/**
 * Geheugen als "16 GB". Een computer meldt iets minder dan de verpakking zegt (een laptop van
 * 8 GB toont 7,4), dus Mondig noemt de dichtstbijzijnde maat erboven.
 */
export function geheugenTekst(bytes: number | null): string | null {
  if (bytes === null) return null;
  const gib = bytes / 1024 ** 3;
  const maat = GEHEUGENMATEN.find((m) => m >= gib - 0.05) ?? Math.round(gib);
  return `${String(maat)} GB`;
}

/** Eén zin over wat Mondig las: geheugen, processor, videokaart. */
export function computerZin(info: Computer, b: Berichten): string {
  const delen: string[] = [];
  const geheugen = geheugenTekst(info.geheugenBytes);
  if (geheugen) delen.push(b.computerGeheugen(geheugen));
  if (info.kernen !== null) delen.push(b.computerKernen(info.kernen));
  const kaart = info.videokaarten?.find((k) => k.soort === 'los') ?? info.videokaarten?.[0];
  if (kaart) {
    delen.push(
      kaart.soort === 'los' ? b.computerKaart(kaart.naam) : b.computerKaartIngebouwd(kaart.naam)
    );
  } else if (info.videokaarten !== null) {
    delen.push(b.computerGeenKaart);
  }
  return delen.join(', ');
}

/** Wat het advies over de videokaart zegt, in gewone taal. */
export function videokaartAdviesTekst(advies: Advies, b: Berichten, taal: 'nl' | 'en'): string {
  const { stand, keerSneller } = advies.videokaart;
  switch (stand) {
    case 'geen':
      return b.adviesKaartGeen;
    case 'ingebouwd':
      return b.adviesKaartIngebouwd;
    case 'testen':
      return b.adviesKaartTesten;
    case 'aanzetten': {
      const keer = keerSneller ?? 0;
      const getal = (keer >= 10 ? Math.round(keer) : Math.round(keer * 10) / 10).toLocaleString(
        taal === 'nl' ? 'nl-NL' : 'en-GB',
        { maximumFractionDigits: 1 }
      );
      return b.adviesKaartAanzetten(getal);
    }
    case 'laten':
      return b.adviesKaartLaten;
    case 'faalde':
      return b.adviesKaartFaalde;
  }
}

/** Het model waarmee Mondig standaard dicteert (hetzelfde als `STANDAARD_MODEL` in Rust). */
export const STANDAARD_MODEL = 'parakeet-ultra-q8';

/**
 * Een advies zonder metingen, voor als het uitlezen van de computer blijft falen: alleen het
 * standaardmodel, voor dicteren en voor bestanden, en niets over de videokaart.
 */
export function standaardAdvies(): ComputerInfo {
  return {
    computer: { geheugenBytes: null, kernen: null, vrijBytes: null, videokaarten: null },
    advies: {
      geheugen: 'onbekend',
      dicteren: STANDAARD_MODEL,
      bestanden: STANDAARD_MODEL,
      bestandenApart: false,
      krap: false,
      tekortDicterenBytes: 0,
      tekortBeideBytes: 0,
      videokaart: { stand: 'geen', keerSneller: null }
    },
    dicterenInGebruik: STANDAARD_MODEL,
    bestandenInGebruik: STANDAARD_MODEL
  };
}

/** Zet een stap van de wizard in het logboek (alleen de vaste naam; Rust negeert de rest). */
export async function wizardStapLog(stap: string): Promise<void> {
  try {
    await invoke<undefined>('wizard_stap_log', { stap });
  } catch {
    // Het logboek mag de wizard nooit in de weg zitten.
  }
}
