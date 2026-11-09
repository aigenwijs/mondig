import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { formatDuur } from './duur';
import type { Berichten } from './i18n/typen';
import type { Foutmelding, Toestand } from './taak';
import type { Markering, PersoonStand } from './verslag';

// Het scherm Bestanden (sprint 4-B): de commando's en events uit app/src-tauri/src/bestanden.rs
// en de teksten van een rij. Gesleepte bestanden komen via Rust binnen; de interface geeft
// nooit zelf een pad door.

/** Een bestand in de lijst. */
export interface BestandInfo {
  taak: number;
  naam: string;
  video: boolean;
  /** Duur volgens de container; `null` als het bestand niet te lezen was. */
  duurMs: number | null;
  toestand: Toestand;
  /** Hoe lang het uitschrijven duurde, als het klaar is. */
  verwerkingMs: number | null;
  heeftTranscript: boolean;
  /** De naam van het laatst opgeslagen bestand (sprint 4-C), of `null`. */
  opgeslagen: string | null;
  /** Is het transcript na het opslaan nog verbeterd? Dan klopt het opgeslagen bestand niet meer. */
  opgeslagenVerouderd: boolean;
  /** Waarom automatisch opslaan niet lukte, of `null`. */
  bewaarFout: string | null;
  /** Kwam dit bestand vanzelf uit de bewaakte map (B-1)? */
  uitMap: boolean;
  /** Overgeslagen omdat er al een transcript van is (bewaakte map). */
  overgeslagen: boolean;
}

/**
 * De hele stand van het scherm. Het volgnummer loopt bij elke wijziging op, zodat een
 * oudere stand (een laat antwoord) een nieuwere niet terugdraait.
 */
export interface BestandenStand {
  versie: number;
  lijst: BestandInfo[];
  /** Een fout van het laatste toevoegen, bijvoorbeeld bij slepen zonder model. */
  fout: Foutmelding | null;
  /** De laatste drop leverde geen bestand op (geen pad en geen belofte, ADR 0030). */
  nietGepakt: boolean;
}

/**
 * De fout die het scherm bij een stand toont: de fout van het toevoegen, of bij een drop die
 * niets opleverde de uitleg wat wel kan.
 */
export function foutVanStand(stand: BestandenStand, t: Berichten): Foutmelding | null {
  if (stand.fout) return stand.fout;
  return stand.nietGepakt ? { melding: t.bestandenNietGepakt, vervolg: 'geen' } : null;
}

/** Is `nieuw` nieuwer dan de stand met dit volgnummer? */
export function isNieuwer(nieuw: BestandenStand, huidig: number): boolean {
  return nieuw.versie > huidig;
}

/** Eén stuk tekst met tijdcodes. */
export interface TranscriptRegel {
  /** De plek van de regel in het transcript: zo heet de regel in `bestandZetTekst`. */
  index: number;
  startMs: number;
  eindMs: number;
  tekst: string;
  /** "Spreker 1", als de sprekers herkend zijn (4b-D); anders `null` of weggelaten. */
  spreker?: string | null;
  /** De persoonsgegevens in de tekst (V-2), in UTF-16-eenheden. */
  markeringen: Markering[];
  /** De tekst voor Kopiëren: vervangen als "Persoonsgegevens vervangen" aan staat. */
  uitvoer: string;
}

/** Het transcript van één bestand. */
export interface BestandTranscript {
  taak: number;
  naam: string;
  duurMs: number;
  klaarOpMs: number;
  model: string;
  soevereiniteit: 'lokaal' | 'europees' | 'buiten_europa';
  /** Een melding van het filter, zoals "Geen geluid gehoord.", of `null`. */
  melding: string | null;
  /** Wat bij het lezen opviel, zoals een afgebroken bestand, of `null`. */
  waarschuwing: string | null;
  regels: TranscriptRegel[];
  /** Het label van de gebruiker, zoals een zaaknummer. */
  label: string;
  /** Staat het bronbestand er nog, zodat een regel kan worden afgespeeld (V-1)? */
  audioAanwezig: boolean;
  /** De stand van "Persoonsgegevens markeren en vervangen" (V-2). */
  persoon: PersoonStand;
}

/** Laat de gebruiker bestanden kiezen en zet ze in de rij. Geeft de nieuwe stand. */
export function bestandenKies(): Promise<BestandenStand> {
  return invoke<BestandenStand>('bestanden_kies');
}

/** De stand zoals hij nu is, met een fout van het laatste toevoegen. */
export function bestandenStand(): Promise<BestandenStand> {
  return invoke<BestandenStand>('bestanden_stand');
}

/** Annuleert een bestand dat wacht of loopt. */
export function bestandAnnuleer(taak: number): Promise<void> {
  return invoke<undefined>('bestand_annuleer', { taak });
}

/** Haalt een bestand uit de lijst (en annuleert het als het nog loopt). */
export function bestandWeghalen(taak: number): Promise<BestandenStand> {
  return invoke<BestandenStand>('bestand_weghalen', { taak });
}

/** Het transcript, of `null` als het er (nog) niet is. */
export function bestandTranscript(taak: number): Promise<BestandTranscript | null> {
  return invoke<BestandTranscript | null>('bestand_transcript', { taak });
}

/**
 * Verbetert de tekst van één regel (V-1) en geeft de regels zoals ze nu zijn. Een lege tekst laat
 * de oude staan. Kopiëren en alle exports gebruiken daarna de nieuwe tekst.
 */
export function bestandZetTekst(
  taak: number,
  index: number,
  tekst: string
): Promise<TranscriptRegel[]> {
  return invoke<TranscriptRegel[]>('bestand_zet_tekst', { taak, index, tekst });
}

/**
 * Het stuk geluid van het bronbestand tussen twee tijden, als WAV-bytes (V-1). De webview maakt er
 * een blob van; ze krijgt nooit een pad.
 */
export function bestandAudioStuk(
  taak: number,
  beginMs: number,
  eindMs: number
): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('bestand_audio_stuk', { taak, beginMs, eindMs });
}

/** Zet "Persoonsgegevens vervangen" aan of uit voor een transcript (V-2). Geeft het transcript. */
export function bestandPersoonVervangen(taak: number, aan: boolean): Promise<BestandTranscript> {
  return invoke<BestandTranscript>('bestand_persoon_vervangen', { taak, aan });
}

/** Haalt een markering weg (V-2): een gevonden stuk overal, een zelf gemarkeerd stuk helemaal. */
export function bestandPersoonWeghalen(taak: number, waarde: string): Promise<BestandTranscript> {
  return invoke<BestandTranscript>('bestand_persoon_weghalen', { taak, waarde });
}

/** Markeert zelf een stuk tekst (V-2), overal waar het als los stuk voorkomt. */
export function bestandPersoonMarkeer(taak: number, waarde: string): Promise<BestandTranscript> {
  return invoke<BestandTranscript>('bestand_persoon_markeer', { taak, waarde });
}

const GEEN_EVENTS: Promise<UnlistenFn> = Promise.resolve(() => undefined);

/** Elke nieuwe stand van het scherm. */
export function luisterNaarStand(bij: (stand: BestandenStand) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<BestandenStand>('bestanden-gewijzigd', (event) => {
    bij(event.payload);
  });
}

/** `true` zolang er bestanden boven het venster hangen. */
export function luisterNaarSlepen(bij: (boven: boolean) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<boolean>('bestanden-slepen', (event) => {
    bij(event.payload);
  });
}

function tweeCijfers(n: number): string {
  return String(n).padStart(2, '0');
}

/** Een tijdcode in het transcript: "0:07", "12:40", boven het uur "1:02:40" (zie duur.ts). */
export function tijdcode(ms: number): string {
  return formatDuur(ms);
}

/** De duur van een bestand: "2:05", "23:14", "1:02:40". */
export function duurTekst(ms: number): string {
  const totaal = Math.max(0, Math.round(ms / 1000));
  const uren = Math.floor(totaal / 3600);
  const minuten = Math.floor((totaal % 3600) / 60);
  const seconden = totaal % 60;
  if (uren > 0) return `${String(uren)}:${tweeCijfers(minuten)}:${tweeCijfers(seconden)}`;
  return `${String(minuten)}:${tweeCijfers(seconden)}`;
}

/** Hoe lang het uitschrijven duurde: "12 s", "1 min 48 s". */
export function verwerkingTekst(ms: number, t: Berichten): string {
  const totaal = Math.max(1, Math.round(ms / 1000));
  if (totaal < 60) return t.duurSeconden(totaal);
  return t.duurMinutenSeconden(Math.floor(totaal / 60), totaal % 60);
}

/** De naam zonder extensie, voor de kop van het transcript. */
export function naamZonderExtensie(naam: string): string {
  const punt = naam.lastIndexOf('.');
  return punt > 0 ? naam.slice(0, punt) : naam;
}

/**
 * De tweede regel van een bestand in de rij, zonder de duur: "wacht", "43 procent, nog
 * ongeveer 3 minuten", "klaar in 1 min 48 s", of de melding bij een fout.
 */
export function stand(info: BestandInfo, resterend: string | null, t: Berichten): string {
  if (info.overgeslagen) return t.bestandOvergeslagen;
  const toestand = info.toestand;
  switch (toestand.type) {
    case 'wacht':
    case 'luistert':
      return t.bestandWacht;
    case 'verwerkt':
      // Na het uitschrijven de sprekers herkennen (4b-D): het rekenen meldt geen voortgang.
      if (toestand.stap === 'sprekers') return t.vergaderSprekersHerkennen;
      if (toestand.voortgang === null) return t.bestandBezig;
      return resterend === null
        ? t.bestandProcent(toestand.voortgang)
        : t.bestandProcentRest(toestand.voortgang, resterend);
    case 'klaar':
      return info.verwerkingMs === null
        ? t.bestandKlaar
        : t.bestandKlaarIn(verwerkingTekst(info.verwerkingMs, t));
    case 'fout':
      return toestand.melding;
    case 'geannuleerd':
      return t.bestandGeannuleerd;
  }
}

/**
 * Alle tekst van een transcript, een regel per stuk, voor het klembord. Met herkende sprekers
 * staat het label voor de tekst ("Spreker 1: tekst"), zoals in de exports.
 */
export function transcriptTekst(transcript: BestandTranscript): string {
  return transcript.regels
    .map((r) => (r.spreker ? `${r.spreker}: ${r.uitvoer}` : r.uitvoer))
    .join('\n');
}
