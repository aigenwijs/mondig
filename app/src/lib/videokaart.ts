import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Berichten, Taal } from './i18n/typen';

// Dunne laag om de schakelaar "Videokaart gebruiken" (2-11-2026), plus de kleine pure regels
// erachter. De beslisregel van de snelheidstest zelf staat in Rust (`mondig_core::snelheid`).

/** Waarop de motoren werkelijk rekenen. Dezelfde namen als `Rekenstand` in Rust. */
export type Rekenstand = 'processor' | 'videokaart' | 'terugval';

/** Een videokaart die het werkproces zag. */
export interface Videokaart {
  naam: string;
  soort: 'los' | 'ingebouwd';
}

/** Antwoord van `videokaart_info`. */
export interface VideokaartInfo {
  aan: boolean;
  /** De gevonden kaarten, leeg als er geen is; `null` als zoeken niet lukte. */
  kaarten: Videokaart[] | null;
}

/** Antwoord van `snelheidstest_start`. */
export interface Vergelijking {
  oordeel: 'loont' | 'loont_niet' | 'videokaart_faalde';
  /** Hoeveel keer sneller de videokaart is; `null` als hij faalde. */
  keerSneller: number | null;
  processor: number;
  videokaart: number | null;
}

export function videokaartInfo(): Promise<VideokaartInfo> {
  return invoke<VideokaartInfo>('videokaart_info');
}

/** Zet de schakelaar; geldt meteen, zonder herstart. */
export function videokaartZet(aan: boolean): Promise<void> {
  return invoke<undefined>('videokaart_zet', { aan });
}

/** Waarop Mondig nu rekent, of `null` als er nog geen model geladen is. */
export function apparaatInGebruik(): Promise<Rekenstand | null> {
  return invoke<Rekenstand | null>('apparaat_in_gebruik');
}

/** Meet het bestandsmodel op de processor en op de videokaart (ongeveer een halve minuut). */
export function snelheidstestStart(): Promise<Vergelijking> {
  return invoke<Vergelijking>('snelheidstest_start');
}

/** Elke nieuwe stand van "In gebruik". Buiten Tauri gebeurt er niets. */
export function luisterNaarApparaat(bij: (stand: Rekenstand | null) => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen<Rekenstand | null>('apparaat-gewijzigd', (event) => {
    bij(event.payload);
  });
}

/** Elke keer dat de videokaart aan- of uitgaat, ook als iets anders dan de schakelaar dat deed. */
export function luisterNaarVideokaart(bij: (aan: boolean) => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen<boolean>('videokaart-gewijzigd', (event) => {
    bij(event.payload);
  });
}

/** De kaart om te noemen: de eerste losse kaart, anders de eerste die er is. */
export function voorkeurKaart(kaarten: Videokaart[] | null): Videokaart | null {
  if (!kaarten || kaarten.length === 0) return null;
  return kaarten.find((k) => k.soort === 'los') ?? kaarten[0] ?? null;
}

/**
 * Kan de gebruiker de schakelaar omzetten? Alleen met een gevonden videokaart. Staat hij al
 * aan (de kaart is er sindsdien uit), dan mag hij wel uit.
 */
export function isBedienbaar(info: VideokaartInfo): boolean {
  return voorkeurKaart(info.kaarten) !== null || info.aan;
}

/** De beschrijving onder de schakelaar: wat Mondig vond. */
export function kaartTekst(info: VideokaartInfo, b: Berichten): string {
  if (info.kaarten === null) return b.videokaartOnbekend;
  const kaart = voorkeurKaart(info.kaarten);
  if (!kaart) return b.videokaartGeen;
  return kaart.soort === 'los'
    ? b.videokaartGevonden(kaart.naam)
    : b.videokaartIngebouwd(kaart.naam);
}

/** De regel "In gebruik", of `null` als er nog geen motor geladen is. */
export function standTekst(stand: Rekenstand | null, b: Berichten): string | null {
  switch (stand) {
    case 'videokaart':
      return b.inGebruikVideokaart;
    case 'processor':
      return b.inGebruikProcessor;
    case 'terugval':
      return b.inGebruikTerugval;
    case null:
      return null;
  }
}

/** "9" of "1,6": onder de tien met één decimaal (zonder ",0"), daarboven afgerond. */
export function keerGetal(keer: number, taal: Taal): string {
  const afgerond = keer >= 10 ? Math.round(keer) : Math.round(keer * 10) / 10;
  return afgerond.toLocaleString(taal === 'nl' ? 'nl-NL' : 'en-GB', {
    maximumFractionDigits: 1
  });
}

/** De uitkomst van de snelheidstest in gewone taal. */
export function testTekst(uitkomst: Vergelijking, b: Berichten, taal: Taal): string {
  switch (uitkomst.oordeel) {
    case 'loont':
      return b.snelheidLoont(keerGetal(uitkomst.keerSneller ?? 0, taal));
    case 'loont_niet':
      return b.snelheidLoontNiet;
    case 'videokaart_faalde':
      return b.snelheidFaalde;
  }
}

/** De regel in "Versiegegevens kopiëren" (altijd Nederlands, zoals de rest daarvan). */
export function versieRegelApparaat(stand: Rekenstand | null): string {
  switch (stand) {
    case 'videokaart':
      return 'Rekent op: videokaart';
    case 'processor':
      return 'Rekent op: processor';
    case 'terugval':
      return 'Rekent op: processor (de videokaart gaf een fout)';
    case null:
      return 'Rekent op: nog geen model geladen';
  }
}
