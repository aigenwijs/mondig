import { invoke } from '@tauri-apps/api/core';
import type { ModelOverzicht } from './modelstatus';

// Dunne laag om de commando's van het instellingenscherm, plus de kleine pure regels
// erachter. Weigert een commando, dan is de afwijzing een `Foutmelding` (zie
// `foutUitCommando` in taak.ts).

/** Een microfoon in de keuzelijst. */
export interface Microfoon {
  id: string;
  naam: string;
}

/** Antwoord van `microfoon_lijst`. */
export interface MicrofoonLijst {
  microfoons: Microfoon[];
  /** De naam van de microfoon die Windows nu als standaard kiest. */
  standaardNaam: string | null;
  /** De bewaarde keuze (id), als die er is. */
  opgeslagen: string | null;
  /** Is de bewaarde microfoon nu aangesloten? Zo niet, dan geldt de standaard. */
  opgeslagenAanwezig: boolean;
}

/** De microfoons van nu, vers opgehaald. */
export function microfoonLijst(): Promise<MicrofoonLijst> {
  return invoke<MicrofoonLijst>('microfoon_lijst');
}

/** Bewaart de gekozen microfoon, of wist de keuze (`null`: de standaard van Windows). */
export function microfoonKies(id: string | null): Promise<void> {
  return invoke<undefined>('microfoon_kies', { id });
}

/**
 * De keuzes onder "Model in het geheugen houden". Dezelfde sleutels als in Rust; "nooit"
 * heet in de interface "Altijd" (de oude keuze "Altijd klaar").
 */
export const RUSTTIJD_SLEUTELS = ['5m', '15m', '1u', 'nooit'] as const;
export type RusttijdSleutel = (typeof RUSTTIJD_SLEUTELS)[number];

/** De standaard (ADR 0012): vijf minuten. */
export const STANDAARD_RUSTTIJD: RusttijdSleutel = '5m';

/** Is dit een sleutel die Rust kent? */
export function isRusttijdSleutel(waarde: string): waarde is RusttijdSleutel {
  return (RUSTTIJD_SLEUTELS as readonly string[]).includes(waarde);
}

/** De bewaarde rusttijd. */
export async function rusttijdInstelling(): Promise<RusttijdSleutel> {
  const sleutel = await invoke<string>('rusttijd_instelling');
  return isRusttijdSleutel(sleutel) ? sleutel : STANDAARD_RUSTTIJD;
}

/** Bewaart een andere rusttijd; die geldt meteen, zonder herstart. */
export function rusttijdZet(sleutel: RusttijdSleutel): Promise<void> {
  return invoke<undefined>('rusttijd_zet', { sleutel });
}

/** Staat de terugval "Alleen op het klembord zetten" aan? Dan plakt Mondig niet. */
export function alleenKlembordInstelling(): Promise<boolean> {
  return invoke<boolean>('alleen_klembord_instelling');
}

/** Zet de terugval aan of uit; geldt vanaf het volgende dictaat. */
export function alleenKlembordZet(aan: boolean): Promise<void> {
  return invoke<undefined>('alleen_klembord_zet', { aan });
}

/** Staat "Stopwoorden weghalen" aan (ADR 0028)? Standaard wel. */
export function stopwoordenInstelling(): Promise<boolean> {
  return invoke<boolean>('stopwoorden_instelling');
}

/** Zet "Stopwoorden weghalen" aan of uit; geldt vanaf het volgende dictaat of bestand. */
export function stopwoordenZet(aan: boolean): Promise<void> {
  return invoke<undefined>('stopwoorden_zet', { aan });
}

/** De schakelaar "Sprekers herkennen" (4b-D) met de stand van het sprekermodel. */
export interface SprekersInstelling {
  /** Staat de schakelaar aan? Standaard wel. */
  aan: boolean;
  /** Het sprekermodel; downloaden gaat met `model_download_start` en `model.id`. */
  model: ModelOverzicht;
}

/** Staat "Sprekers herkennen" aan, en staat het sprekermodel op de computer? */
export function sprekersInstelling(): Promise<SprekersInstelling> {
  return invoke<SprekersInstelling>('sprekers_instelling');
}

/** Zet "Sprekers herkennen" aan of uit; geldt vanaf de volgende vergadering die uitgeschreven wordt. */
export function sprekersZet(aan: boolean): Promise<void> {
  return invoke<undefined>('sprekers_zet', { aan });
}

/** Houdt het niveau binnen 0 tot 1, voor de breedte van de meter. */
export function meterBreedte(niveau: number): number {
  if (!Number.isFinite(niveau)) return 0;
  return Math.min(1, Math.max(0, niveau));
}

/**
 * De waarde van het keuzemenu: de bewaarde microfoon als die er nog is, anders de lege
 * waarde voor "Standaard van Windows".
 */
export function gekozenWaarde(lijst: MicrofoonLijst): string {
  return lijst.opgeslagen !== null && lijst.opgeslagenAanwezig ? lijst.opgeslagen : '';
}

/** Moet de uitleg "je microfoon is niet aangesloten" erbij staan? */
export function microfoonIsWeg(lijst: MicrofoonLijst): boolean {
  return lijst.opgeslagen !== null && !lijst.opgeslagenAanwezig;
}

/** Zijn twee lijsten gelijk? Dan hoeft het menu niet opnieuw getekend. */
export function zelfdeLijst(a: MicrofoonLijst, b: MicrofoonLijst): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Waar de pil verschijnt. Dezelfde namen als `Plek` in Rust. */
export type PilPositie = 'onder' | 'boven';

/** De bewaarde plek van de pil. */
export async function pilPositieInstelling(): Promise<PilPositie> {
  const plek = await invoke<string>('pil_positie_instelling');
  return plek === 'boven' ? 'boven' : 'onder';
}

/** Bewaart de plek van de pil; geldt vanaf de volgende keer dat hij verschijnt. */
export function pilPositieZet(plek: PilPositie): Promise<void> {
  return invoke<undefined>('pil_positie_zet', { plek });
}

/** Het geluid bij starten en stoppen. */
export interface GeluidInstelling {
  aan: boolean;
  /** 0 tot 100. */
  volume: number;
}

/** De bewaarde geluidsinstelling. */
export function geluidInstelling(): Promise<GeluidInstelling> {
  return invoke<GeluidInstelling>('geluid_instelling');
}

/** Bewaart de geluidsinstelling. Met `proef` klinkt meteen het startgeluid. */
export function geluidZet(instelling: GeluidInstelling, proef: boolean): Promise<void> {
  return invoke<undefined>('geluid_zet', {
    instelling: { aan: instelling.aan, volume: volumeVeilig(instelling.volume) },
    proef
  });
}

/** Houdt een volume binnen 0 tot 100, in hele stappen. */
export function volumeVeilig(volume: number): number {
  if (!Number.isFinite(volume)) return 0;
  return Math.round(Math.min(100, Math.max(0, volume)));
}

/** Start Mondig zonder hoofdvenster, alleen in de systeembalk? */
export function verborgenStartenInstelling(): Promise<boolean> {
  return invoke<boolean>('verborgen_starten_instelling');
}

/** Zet verborgen starten aan of uit; geldt bij de volgende start. */
export function verborgenStartenZet(aan: boolean): Promise<void> {
  return invoke<undefined>('verborgen_starten_zet', { aan });
}

/** Start Mondig bij het aanmelden? De stand komt uit Windows zelf. */
export function autostartInstelling(): Promise<boolean> {
  return invoke<boolean>('autostart_instelling');
}

/** Zet starten bij aanmelden aan of uit. */
export function autostartZet(aan: boolean): Promise<void> {
  return invoke<undefined>('autostart_zet', { aan });
}
