import { invoke } from '@tauri-apps/api/core';
import type { RijStand } from './downloadrij';
import { LIJST_EERSTE, type DictatenAntwoord, type DictatenTermijn } from './dictaten';
import type { DownloadVoortgang, Toestand } from './taak';

// Dunne laag om de Tauri-commando's van het downloadscherm en de proef. Weigert een
// commando, dan is de afwijzing een `Foutmelding` (zie `foutUitCommando` in taak.ts).

/** Een download die nog loopt, voor wie het scherm opnieuw opent. */
export interface LopendeDownload {
  taak: number;
  /** Het id van het model dat binnenkomt. */
  model: string;
  toestand: Toestand;
  voortgang: DownloadVoortgang | null;
}

/**
 * Stopt de download van een model (of zonder model: de download die nu binnenkomt). Wacht het
 * model nog in de rij, dan komt het niet meer aan de beurt; een onderbroken download wordt niet
 * meer hervat. Het deelbestand blijft staan.
 */
export function downloadStop(model?: string): Promise<void> {
  return invoke<undefined>('download_stop', { model: model ?? null });
}

/** De hele downloadrij met zijn volgnummer. */
export function downloadsLijst(): Promise<RijStand> {
  return invoke<RijStand>('downloads_lijst');
}

/** De lopende download, of `null` als er geen is. */
export function downloadLopend(): Promise<LopendeDownload | null> {
  return invoke<LopendeDownload | null>('download_lopend');
}

/** Het laatste dictaat, als er een is. Alleen in het geheugen van de app bewaard. */
export function laatsteDictaat(): Promise<string | null> {
  return invoke<string | null>('laatste_dictaat');
}

/**
 * De termijn van "Dictaten bewaren" en een deel van de bewaarde dictaten, nieuwste eerst (ADR
 * 0021): `limiet` dictaten vanaf plaats `begin`, plus het totaal. Zo hoeft niet alle tekst over
 * de brug.
 */
export function dictatenStand(begin = 0, limiet = LIJST_EERSTE): Promise<DictatenAntwoord> {
  return invoke<DictatenAntwoord>('dictaten_stand', { begin, limiet });
}

/**
 * Hoeveel bewaarde dictaten er wegvallen als deze termijn gekozen wordt. Wist niets; voor de
 * bevestigvraag bij een kortere of strengere stand.
 */
export function dictatenWegval(termijn: DictatenTermijn): Promise<number> {
  return invoke<number>('dictaten_wegval', { termijn });
}

/**
 * Bewaart een andere termijn. Korter wist meteen wat ouder is (of, bij Laatste 10, alles
 * behalve de tien nieuwste); Niet bewaren wist alle bewaarde dictaten. De bevestigvraag stelt de
 * interface zelf, alleen als er dictaten wegvallen.
 */
export function dictatenBewarenZet(termijn: DictatenTermijn): Promise<void> {
  return invoke<undefined>('dictaten_bewaren_zet', { termijn });
}

/** Verwijdert één bewaard dictaat, direct. */
export function dictaatVerwijder(id: string): Promise<void> {
  return invoke<undefined>('dictaat_verwijder', { id });
}

/** Wist alle bewaarde dictaten, en het laatste dictaat in het geheugen. */
export function dictatenWisAlles(): Promise<void> {
  return invoke<undefined>('dictaten_wis_alles');
}

/** Annuleert een dictaat van de sneltoets: tijdens het opnemen of het uitschrijven. */
export function dictaatAnnuleer(taak: number): Promise<void> {
  return invoke<undefined>('dictaat_annuleer', { taak });
}

/** Stopt een dictaat van de sneltoets en schrijft het uit (de knop Stoppen in de pil). */
export function dictaatStop(taak: number): Promise<void> {
  return invoke<undefined>('dictaat_stop', { taak });
}

/** Opent de juiste Windows-instelling voor de microfoon (privacy als Windows hem blokkeert). */
export function microfooninstellingOpen(): Promise<void> {
  return invoke<undefined>('microfooninstelling_open');
}

/** Het recht op toegankelijkheid op de Mac; Windows kent het niet (`niet_nodig`). */
export type ToegangStand = 'niet_nodig' | 'toegestaan' | 'geweigerd';

/** Mag Mondig op de Mac toetsen sturen (en dus plakken)? */
export function toegankelijkheidStand(): Promise<ToegangStand> {
  return invoke<ToegangStand>('toegankelijkheid_stand');
}

/** Zet Mondig in de lijst bij Toegankelijkheid en opent die instelling (alleen op de Mac). */
export function toegankelijkheidOpen(): Promise<void> {
  return invoke<undefined>('toegankelijkheid_open');
}

/** Toont de pil met deze maat in CSS-pixels; Rust kiest het scherm en de plek. */
export function pilToon(breedte: number, hoogte: number): Promise<void> {
  return invoke<undefined>('pil_toon', { breedte, hoogte });
}

/** Wat een pil die net laadt moet weten: events van daarvoor zijn gemist. */
export interface PilStand {
  taak: number | null;
  luistert: boolean;
}

/** Het laatste dictaat van de sneltoets en of de opname nog loopt. */
export function pilStand(): Promise<PilStand> {
  return invoke<PilStand>('pil_stand');
}

/** Verbergt de pil. */
export function pilVerberg(): Promise<void> {
  return invoke<undefined>('pil_verberg');
}

/**
 * Er is op de pil geklikt. Op de Mac activeert dat Mondig; Rust zet dan het programma waar je
 * typte weer vooraan, zodat het volgende dictaat daar plakt. Elders doet het niets.
 */
export function pilKlik(): Promise<void> {
  return invoke<undefined>('pil_klik');
}

/** Opent het venster van Mondig bij het spraakmodel (vervolgactie vanuit de pil). */
export function pilOpenMondig(): Promise<void> {
  return invoke<undefined>('pil_open_mondig');
}
