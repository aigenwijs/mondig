import { invoke } from '@tauri-apps/api/core';
import type { Berichten } from './i18n/typen';
import type { VergaderingStand } from './vergadering';

// Notulen (T3, ADR 0024, 0025 en 0026): de commando's uit app/src-tauri/src/notulen.rs en
// vergadering/notulen_maken.rs, en de pure hulpjes voor Instellingen en het tabblad Notulen.
// De sleutel van Mistral gaat alleen van de interface naar Rust (`notulenSleutelZet`) en komt
// nooit terug: de interface weet alleen of er een sleutel staat.

/** Waar de tekst van een verslag heen gaat om notulen te maken. */
export type NotulenAanbieder = 'mistral' | 'ollama';

/** Het standaardmodel bij Mistral: Nauwkeurig. */
export const MISTRAL_NAUWKEURIG = 'mistral-large-latest';
/** De tweede keuze bij Mistral: Sneller. */
export const MISTRAL_SNEL = 'mistral-small-latest';

/** De instellingen van Notulen, zoals Rust ze geeft. Bevat nooit een sleutel. */
export interface NotulenInstellingen {
  /** `null`: niet ingesteld, dan maakt Mondig geen notulen. */
  aanbieder: NotulenAanbieder | null;
  mistralModel: string;
  ollamaModel: string | null;
  /** Het vinkje "Training staat uit voor deze sleutel." */
  trainingUit: boolean;
  /** Staat er een sleutel voor Mistral in de sleutelbos? */
  sleutelStaat: boolean;
  /** Wat er met de sleutelbos mis is, in gewone taal. */
  sleutelFout: string | null;
}

/** Draait Ollama op deze computer, en welke modellen heeft het? */
export interface OllamaStand {
  draait: boolean;
  modellen: string[];
  melding: string | null;
}

/** Waar een punt in het verslag gezegd is. */
export interface NotulenBron {
  /** Staat het citaat echt in het verslag? */
  gevonden: boolean;
  /** De beurt (vanaf 0) waar de bronknop naartoe springt; `null` als dat niet kan. */
  beurt: number | null;
  ms: number | null;
  /** Het citaat dat het model gaf. */
  citaat: string;
}

export interface NotulenBesluit {
  /** Het vaste nummer van het punt: commando's gaan op dit nummer, niet op de plek in de lijst. */
  id: number;
  tekst: string;
  bron: NotulenBron;
}

export interface NotulenActiepunt {
  id: number;
  wat: string;
  wie: string;
  wanneer: string;
  wieNietTeruggevonden: boolean;
  wanneerNietTeruggevonden: boolean;
  bron: NotulenBron;
}

/** De notulen van een verslag. */
export interface NotulenInfo {
  samenvatting: string;
  besluiten: NotulenBesluit[];
  actiepunten: NotulenActiepunt[];
  aanbieder: string;
  model: string;
  momentMs: number;
  taal: 'nl' | 'en';
  /** Gemaakt bij een andere versie van het verslag: de bronnen zijn niet klikbaar. */
  vorigeVersie: boolean;
  gevonden: number;
  nietGevonden: number;
}

/** De voortgang van het maken: hoeveel delen klaar zijn en hoeveel het er worden. */
export interface NotulenVoortgang {
  klaar: number;
  /** 0 als het aantal delen nog niet bekend is. */
  totaal: number;
}

/** De notulen en het maken ervan, in de stand van het verslag. */
export interface NotulenStand {
  info: NotulenInfo | null;
  /** Notulen die deze versie niet kan lezen; ze blijven bewaard. */
  onleesbaar: boolean;
  maken: NotulenVoortgang | null;
}

/** Een besluit of een actiepunt. */
export type PuntSoort = 'besluit' | 'actiepunt';

// ---------------------------------------------------------------------------
// Commando's
// ---------------------------------------------------------------------------

export function notulenInstellingen(): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_instellingen');
}

export function notulenZetAanbieder(
  aanbieder: NotulenAanbieder | null
): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_zet_aanbieder', { aanbieder });
}

export function notulenZetModel(
  aanbieder: NotulenAanbieder,
  model: string
): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_zet_model', { aanbieder, model });
}

export function notulenZetTraining(uit: boolean): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_zet_training', { uit });
}

/** Bewaart de sleutel in de sleutelbos. De waarde gaat alleen heen; terug komt of er een staat. */
export function notulenSleutelZet(waarde: string): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_sleutel_zet', { waarde });
}

export function notulenSleutelWis(): Promise<NotulenInstellingen> {
  return invoke<NotulenInstellingen>('notulen_sleutel_wis');
}

/** Test de sleutel met een lichte aanvraag; er gaat geen verslagtekst mee. */
export function notulenSleutelTest(): Promise<void> {
  return invoke<undefined>('notulen_sleutel_test');
}

export function notulenOllamaStand(): Promise<OllamaStand> {
  return invoke<OllamaStand>('notulen_ollama_stand');
}

/** Wat de gebruiker bevestigde: waar de tekst heen gaat en of persoonsgegevens vervangen worden. */
export interface Bevestiging {
  aanbieder: NotulenAanbieder;
  vervangen: boolean;
}

/**
 * Maakt notulen van het open verslag, in de taal van de interface. Rust weigert als de
 * bevestiging niet meer klopt met de instelling en de stand van Vervangen, en overschrijft
 * onleesbare notulen alleen met `overschrijf`.
 */
export function vergaderingNotulenMaken(
  taal: 'nl' | 'en',
  bevestigd: Bevestiging,
  overschrijf: boolean
): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_notulen_maken', { taal, bevestigd, overschrijf });
}

/** Geldt een eerdere bevestiging nog voor dit verslag, deze aanbieder en deze stand van Vervangen? */
export function bevestigingGeldt(
  eerder: (Bevestiging & { taak: number }) | null,
  taak: number,
  aanbieder: NotulenAanbieder | null,
  vervangen: boolean
): boolean {
  return (
    eerder !== null &&
    aanbieder !== null &&
    eerder.taak === taak &&
    eerder.aanbieder === aanbieder &&
    eerder.vervangen === vervangen
  );
}

/** Stopt het maken: het verslag blijft zoals het was. */
export function vergaderingNotulenStoppen(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_notulen_stoppen');
}

export function vergaderingNotulenPuntAanpassen(
  soort: PuntSoort,
  id: number,
  tekst: string,
  wie: string | null,
  wanneer: string | null
): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_notulen_punt_aanpassen', {
    soort,
    id,
    tekst,
    wie,
    wanneer
  });
}

export function vergaderingNotulenPuntWeghalen(
  soort: PuntSoort,
  id: number
): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_notulen_punt_weghalen', { soort, id });
}

/** Zet alleen de notulen op het klembord. */
export function vergaderingNotulenKopieer(): Promise<void> {
  return invoke<undefined>('vergadering_notulen_kopieer');
}

// ---------------------------------------------------------------------------
// Pure hulpjes
// ---------------------------------------------------------------------------

/** Wat er nog ontbreekt voor Notulen maken; `ok` als het kan. */
export type NotulenKlaar = 'ok' | 'niet-ingesteld' | 'training' | 'sleutel' | 'model';

/**
 * Kan Mondig notulen maken met deze instellingen? Dezelfde regels als Rust (`bepaal_start`):
 * bij Mistral zijn het vinkje over training én een sleutel verplicht, bij Ollama een model.
 * Rust weigert ook als de interface dit verkeerd inschat; dit is voor de juiste knop en tekst.
 */
export function notulenKlaar(instellingen: NotulenInstellingen | null): NotulenKlaar | null {
  if (instellingen === null) return null;
  if (instellingen.aanbieder === null) return 'niet-ingesteld';
  if (instellingen.aanbieder === 'mistral') {
    if (!instellingen.trainingUit) return 'training';
    if (!instellingen.sleutelStaat) return 'sleutel';
    return 'ok';
  }
  return instellingen.ollamaModel === null || instellingen.ollamaModel === '' ? 'model' : 'ok';
}

/** De naam van een aanbieder met zijn label, zoals de gebruiker hem ziet. */
export function aanbiederNaam(
  aanbieder: string,
  t: Pick<Berichten, 'notulenKeuzeMistral' | 'notulenKeuzeOllama'>
): string {
  return aanbieder === 'mistral'
    ? t.notulenKeuzeMistral
    : aanbieder === 'ollama'
      ? t.notulenKeuzeOllama
      : aanbieder;
}

/** Hoe een model heet in "Gemaakt met ...": de bekende Mistral-modellen bij hun naam. */
export function modelNaam(model: string): string {
  if (model === MISTRAL_NAUWKEURIG) return 'Mistral Large';
  if (model === MISTRAL_SNEL) return 'Mistral Small';
  return model;
}

/**
 * Waar de notulen gemaakt zijn, voor de regel onderaan: bij Mistral "Mistral Large (Europees)", bij
 * Ollama "<model> op deze computer (Lokaal)". Zo zegt de regel zelf waar de tekst heen ging, los van
 * "op deze computer" in de gegevensregel van het verslag.
 */
export function plekTekst(
  aanbieder: string,
  model: string,
  t: Pick<Berichten, 'notulenPlekEuropees' | 'notulenPlekLokaal'>
): string {
  return aanbieder === 'ollama'
    ? t.notulenPlekLokaal(modelNaam(model))
    : t.notulenPlekEuropees(modelNaam(model));
}

/**
 * Laat de bronknop naar het verslag springen. Geeft `true` als er gesprongen is; een bron die niet
 * gevonden is of bij een vorige versie van het verslag hoort, heeft geen beurt en springt nergens heen.
 */
export function springNaarBron(
  bron: Pick<NotulenBron, 'beurt'>,
  spring: (beurt: number) => void
): boolean {
  if (bron.beurt === null) return false;
  spring(bron.beurt);
  return true;
}

/**
 * Bewaart een sleutel en laat het veld leeg. Gaat het goed, dan is het veld leeg en is er geen
 * fout; gaat het mis, dan blijft de tekst staan, zodat de gebruiker hem kan verbeteren.
 */
export async function bewaarSleutel(
  waarde: string,
  zet: (waarde: string) => Promise<NotulenInstellingen>
): Promise<{ veld: string; instellingen: NotulenInstellingen | null; fout: unknown }> {
  try {
    const instellingen = await zet(waarde);
    return { veld: '', instellingen, fout: null };
  } catch (fout) {
    return { veld: waarde, instellingen: null, fout };
  }
}

/** Hoeveel milliseconden de gesprongen regel oplicht. */
export const OPLICHT_MS = 2500;

/** Het deel dat bezig is, bij het maken: "Deel 2 van 4", en zonder bekend aantal één zin. */
export function deelTekst(
  voortgang: NotulenVoortgang,
  t: Pick<Berichten, 'notulenBezigDeel' | 'notulenBezigLeest'>
): string {
  if (voortgang.totaal <= 1) return t.notulenBezigLeest;
  return t.notulenBezigDeel(Math.min(voortgang.klaar + 1, voortgang.totaal), voortgang.totaal);
}

/** Het aandeel dat klaar is, van 0 tot 100, of `null` als het aantal delen nog onbekend is. */
export function voortgangProcent(voortgang: NotulenVoortgang): number | null {
  if (voortgang.totaal <= 1) return null;
  return Math.round((Math.min(voortgang.klaar, voortgang.totaal) / voortgang.totaal) * 100);
}
