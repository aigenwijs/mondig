import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { BewaarUitslag, Formaat } from './bewaren';
import { formatDuur } from './duur';
import type { Berichten } from './i18n/typen';
import type { NotulenStand } from './notulen';
import type { Foutmelding, Stap } from './taak';
import type { Markering, PersoonStand } from './verslag';

// Het scherm Vergadering (sprint 4b-C): de commando's
// en events uit app/src-tauri/src/vergadering.rs, en de pure hulpjes voor het scherm
// (routes/vergadering). De interface geeft nooit een pad door: een bewaarde opname heet bij haar
// `id` uit de stand.

/**
 * Online (microfoon en het geluid van de computer), in de ruimte (alleen de microfoon) of
 * Alleen ik (alleen de microfoon, het verslag heet een notitie; fase UX-D).
 */
export type Vergadersoort = 'online' | 'ruimte' | 'alleen';

/** De soorten in de volgorde van de schakelaar op het startscherm. */
export const SOORTEN: readonly Vergadersoort[] = ['online', 'ruimte', 'alleen'];

/** Wie er spreekt: het microfoonspoor of het geluid van de computer. */
export type Kant = 'jij' | 'anderen';

/**
 * De laatste wijziging aan de sprekers (een naam, samenvoegen, een beurt aan een andere spreker),
 * zolang die met "Ongedaan maken" terug kan (Rust: `Sprekerstap`).
 */
export interface Sprekerstap {
  /** Een nummer per stap: een wijziging die niets deed, laat de oude stap (en zijn nummer) staan. */
  volgnummer: number;
  /** De kant waar het gebeurde. */
  kant: Kant;
  /**
   * De sprekers die door een naam samengingen, als nummer; `null` is de kant zonder nummer
   * ("Anderen"). Leeg als er niets samenging.
   */
  samengevoegd: (number | null)[];
  /** Bij een naam: de naam zoals hij nu in het verslag staat; `null` is de standaardnaam. */
  naam: string | null;
  /** Bij een naam: het sprekernummer daarna, voor de standaardnaam ("Terug naar Spreker 2"). */
  nummer: number | null;
}

/**
 * De stap die een wijziging maakte: de nieuwe `sprekerstap` als Rust echt een nieuwe stap maakte
 * (een ander volgnummer dan `voor`), anders `null`. Zo zet een wijziging die niets deed (Opslaan
 * zonder andere naam) geen melding neer die bij een oudere stap hoort.
 */
export function nieuweStap(voor: number | null, na: Sprekerstap | null): Sprekerstap | null {
  return na !== null && na.volgnummer !== voor ? na : null;
}

/** Het niveau per spoor tijdens de opname, van 0 (stilte) tot 1. */
export interface Niveaus {
  microfoon: number;
  /** `null` zonder geluid van de computer (fysieke vergadering, Alleen ik, of geen afspeelapparaat). */
  systeem: number | null;
}

/** Een opname die nog in de tijdelijke map staat. */
export interface OpnameInfo {
  /** De mapnaam: zo heet de opname in de commando's. */
  id: string;
  soort: Vergadersoort;
  beginMs: number;
  /** De duur; bij een afgebroken opname geschat. */
  duurMs: number | null;
  /** Netjes afgesloten ("Later uitschrijven"); anders afgebroken, eerst herstellen. */
  afgesloten: boolean;
  /** Mondig kan niet nagaan of deze opname nog loopt (het slot is niet te lezen). */
  onzeker: boolean;
  /** Wanneer Mondig de opname vanzelf wist (milliseconden sinds 1970); `null` als dat niet bekend is. */
  wistMs: number | null;
}

/** Een bewaard verslag in de lijst "Eerder opgenomen" (ADR 0022). */
export interface VerslagInfo {
  /** De mapnaam: zo heet het verslag in de commando's. */
  id: string;
  /** De naam die de gebruiker het gaf; zonder naam maakt het scherm de titel uit soort en begin. */
  naam: string | null;
  soort: Vergadersoort;
  beginMs: number;
  duurMs: number;
  /** Hoeveel sprekers het verslag heeft, als ze herkend zijn. */
  sprekers: number | null;
  /** Wanneer Mondig het verslag vanzelf wist (milliseconden sinds 1970); `null` als dat niet bekend is. */
  wistMs: number | null;
  /** Staat de opname er nog, zodat het verslag opnieuw kan worden uitgeschreven (ADR 0023)? */
  opnameAanwezig: boolean;
}

/**
 * Hoe een spraakmodel heet bij opnieuw uitschrijven (ADR 0023): bij zijn rol, nooit bij zijn naam
 * (N-B). Dezelfde namen als `Modelrol` in Rust.
 */
export type Modelrol = 'snel' | 'nauwkeurig' | 'extra_nauwkeurig';

/** Het opnieuw uitschrijven dat nu loopt. */
export interface OpnieuwStand {
  rol: Modelrol | null;
  voortgang: number | null;
  /** De resterende tijd in milliseconden, als die te schatten is. */
  restMs: number | null;
}

/** Wat er na het opnieuw uitschrijven veranderde, voor de melding. */
export interface NaOpnieuw {
  rol: Modelrol | null;
  /** Hoeveel regels er veranderden. */
  veranderd: number;
  /** Hoeveel eigen markeringen niet meer in de tekst staan. */
  verdwenen: number;
}

/** Een model om het verslag opnieuw mee uit te schrijven. */
export interface OpnieuwKeuze {
  modelId: string;
  rol: Modelrol;
  /** Staat het model op deze computer? Anders eerst downloaden bij Modellen. */
  aanwezig: boolean;
  grootteBytes: number;
  /** Hoe lang het voor dit verslag ongeveer duurt; `null` als Mondig dat nog niet heeft gemeten. */
  duurMs: number | null;
}

/** Wat de dialoog "Opnieuw uitschrijven" toont. */
export interface OpnieuwKeuzes {
  /** De rol van het model waarmee het verslag nu is uitgeschreven. */
  rol: Modelrol | null;
  keuzes: OpnieuwKeuze[];
}

/** Een beurt in het verslag. */
export interface BeurtInfo {
  /** De plek in het verslag, voor `vergaderingNaam`. */
  index: number;
  kant: Kant;
  /**
   * Welke spreker van die kant (vanaf 1) als de sprekers herkend zijn (4b-D), anders `null`.
   * Zonder eigen naam toont de interface `t().vergaderSpreker(nummer)`.
   */
  sprekernummer: number | null;
  /** "Jij", "Anderen", "Spreker 1" of een gegeven naam; `null` in de ruimte zonder sprekers. */
  spreker: string | null;
  /**
   * `spreker` zoals hij op het scherm staat: met Vervangen aan en een gemarkeerd gegeven in de
   * naam staat hier `[GEGEVEN]` enzovoort, zoals in Kopiëren en Opslaan. Naam geven begint met
   * de echte `spreker`.
   */
  sprekerGetoond: string | null;
  /** Heeft de gebruiker deze beurt een eigen naam gegeven? */
  eigenNaam: boolean;
  startMs: number;
  eindMs: number;
  tekst: string;
  /** De persoonsgegevens in de tekst (V-2), in UTF-16-eenheden. */
  markeringen: Markering[];
  /** Veranderd bij het laatste opnieuw uitschrijven (ADR 0023): de regel wordt even gemarkeerd. */
  veranderd: boolean;
}

/** Wat er nu gebeurt. */
export type StandToestand =
  | { type: 'klaar' }
  | { type: 'starten'; soort: Vergadersoort }
  | {
      /** Mondig stopt, herstelt of zet de opname in de wachtrij; zo meteen volgt `uitschrijven` of `klaar`. */
      type: 'voorbereiden';
      soort: Vergadersoort;
      beginMs: number;
      duurMs: number | null;
      /** Wordt een afgebroken opname hersteld? */
      herstellen: boolean;
    }
  | {
      type: 'neemtOp';
      soort: Vergadersoort;
      /** Begin van de opname, in milliseconden sinds 1970. */
      beginMs: number;
      duurMs: number;
      /** Het laatst gemeten niveau; live via `luisterNaarNiveau`. */
      niveaus: Niveaus | null;
      /** De waarschuwing als er al een minuut geen geluid van de computer komt. */
      systeemStil: string | null;
      /** Wat er tijdens de opname gebeurde, zoals een bron die uitviel. */
      meldingen: string[];
    }
  | {
      type: 'uitschrijven';
      /** De taak in de `taak`-events (soort `vergadering`), voor de resterende tijd. */
      taak: number;
      soort: Vergadersoort;
      beginMs: number;
      duurMs: number | null;
      voortgang: number | null;
      /**
       * Uitschrijven, of daarna sprekers herkennen (4b-D). Bij `sprekers` meldt het rekenen zich
       * niet, dus dan toont het scherm de stap in plaats van een percentage.
       */
      stap?: Stap;
      model: string;
    }
  | {
      type: 'verslag';
      taak: number;
      soort: Vergadersoort;
      beginMs: number;
      duurMs: number;
      model: string;
      soevereiniteit: 'lokaal' | 'europees' | 'buiten_europa';
      meldingen: string[];
      /** Hoeveel uitingen van Jij als echo zijn weggelaten. */
      weggelaten: number;
      beurten: BeurtInfo[];
      /** Staat de opname er nog, zodat een beurt kan worden afgespeeld (V-1)? */
      audioAanwezig: boolean;
      /** De stand van "Persoonsgegevens markeren en vervangen" (V-2). */
      persoon: PersoonStand;
      /** De naam die de gebruiker het verslag gaf; `null`: het scherm maakt de titel zelf. */
      naam: string | null;
      /**
       * `naam` zoals de kop hem toont: met Vervangen aan en een gemarkeerd gegeven erin staat hier
       * `[GEGEVEN]` enzovoort, zoals in de bestandsnaam en de export. Naam wijzigen begint met `naam`.
       */
      naamGetoond: string | null;
      /** Wanneer Mondig het verslag vanzelf wist (milliseconden sinds 1970); `null` als dat niet bekend is. */
      wistMs: number | null;
      /** Kon Mondig de laatste wijzigingen niet bewaren? Dan weigert Sluiten, tenzij je toch sluit. */
      bewarenMislukt: boolean;
      /** De id van het model waarmee het verslag is uitgeschreven. */
      modelId: string;
      /** Staat de opname er nog, zodat het verslag opnieuw kan worden uitgeschreven (ADR 0023)? */
      opnameAanwezig: boolean;
      /**
       * De rol van het model voor de gegevensregel (N-B: de rol, de naam hooguit als tooltip);
       * `null` als het geen spraakmodel uit de catalogus is.
       */
      modelRol: Modelrol | null;
      /** Hoeveel regels de gebruiker zelf verbeterde. */
      verbeterd: number;
      /** Hoeveel geluid het uitschrijven verwerkt, in milliseconden (duur keer aantal sporen). */
      geluidMs: number;
      /** Het opnieuw uitschrijven dat nu loopt; dan weigert elke wijziging. */
      opnieuw: OpnieuwStand | null;
      /** Wat er na het laatste opnieuw uitschrijven veranderde. */
      naOpnieuw: NaOpnieuw | null;
      /** Kan "Vorige versie terugzetten" nog (tot het verslag sluit of opnieuw wordt uitgeschreven)? */
      vorigeVersie: boolean;
      /** De laatste wijziging aan de sprekers, zolang die ongedaan kan worden gemaakt. */
      sprekerstap: Sprekerstap | null;
      /** De notulen van dit verslag en het maken ervan (T3, ADR 0026). */
      notulen: NotulenStand;
    };

/** De hele stand van het scherm, met een volgnummer dat bij elke wijziging oploopt. */
export interface VergaderingStand {
  versie: number;
  toestand: StandToestand;
  /** Opnames die nog uitgeschreven of hersteld kunnen worden, oudste eerst. */
  opnames: OpnameInfo[];
  /** Bewaarde verslagen ("Eerder opgenomen"), nieuwste eerst. */
  verslagen: VerslagInfo[];
  /** De laatste fout (starten, uitschrijven, wissen), tot de volgende handeling. */
  fout: Foutmelding | null;
  /** De laatst gekozen soort, voor de keuze bij de start. */
  laatsteSoort: Vergadersoort;
}

/** Een luidspreker of koptelefoon voor de keuze "Geluid van". */
export interface AfspeelInfo {
  id: string;
  naam: string;
  standaard: boolean;
}

/** Is `nieuw` nieuwer dan de stand met dit volgnummer? */
export function isNieuwer(nieuw: VergaderingStand, huidig: number): boolean {
  return nieuw.versie > huidig;
}

/** Loopt er een opname (of gaat er een beginnen)? */
export function neemtOp(stand: VergaderingStand): boolean {
  return stand.toestand.type === 'neemtOp' || stand.toestand.type === 'starten';
}

/** De stand zoals hij nu is; de lijst met opnames wordt opnieuw gelezen. */
export function vergaderingStand(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_stand');
}

/** De afspeelapparaten van nu, voor "Geluid van". */
export function vergaderingAfspeelapparaten(): Promise<AfspeelInfo[]> {
  return invoke<AfspeelInfo[]>('vergadering_afspeelapparaten');
}

/**
 * Start een opname. `null` is de bewaarde microfoon (anders de standaard) en het
 * standaardafspeelapparaat. Het openen kan seconden duren.
 */
export function vergaderingStart(
  soort: Vergadersoort,
  microfoon: string | null,
  afspeelapparaat: string | null
): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_start', { soort, microfoon, afspeelapparaat });
}

/**
 * Heeft "Opname starten" in het systeemvak om de vraag "Vertel het de anderen" gevraagd? Geeft de
 * soort waarvoor, één keer (en `null` als er geen verzoek is of het ouder is dan 30 seconden), zodat
 * het ook werkt als dit scherm er nog niet stond.
 */
export function vergaderingStartGevraagd(): Promise<Vergadersoort | null> {
  return invoke<Vergadersoort | null>('vergadering_start_gevraagd');
}

/** Stopt de opname; met `uitschrijven` meteen in de wachtrij, anders voor later. */
export function vergaderingStop(uitschrijven: boolean): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_stop', { uitschrijven });
}

/** "Later uitschrijven": het uitschrijven stopt en de opname blijft staan. */
export function vergaderingLater(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_later');
}

/** Wist een opname: zonder `id` die van nu (opname, uitschrijven of verslag), anders uit de lijst. */
export function vergaderingWeggooien(id: string | null): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_weggooien', { id });
}

/** Schrijft een netjes afgesloten opname uit de lijst uit. */
export function vergaderingUitschrijven(id: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_uitschrijven', { id });
}

/** Herstelt een afgebroken opname uit de lijst en schrijft haar uit. */
export function vergaderingHerstel(id: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_herstel', { id });
}

/**
 * Geeft de spreker van een beurt een naam, voor al zijn beurten; een lege naam zet terug naar
 * "Jij", "Anderen" of "Spreker 1". Heeft een andere spreker van die kant dezelfde naam, dan gaan
 * ze samen (`sprekerstap.samengevoegd` in de nieuwe stand).
 */
export function vergaderingNaam(index: number, naam: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_naam', { index, naam });
}

/** Maakt de laatste wijziging aan de sprekers ongedaan (een naam, samenvoegen, een andere spreker). */
export function vergaderingStapTerug(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_stap_terug');
}

/**
 * Verbetert de tekst van een beurt (V-1). Een lege tekst laat de oude staan. Kopiëren en alle
 * exports gebruiken daarna de nieuwe tekst.
 */
export function vergaderingZetTekst(index: number, tekst: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_zet_tekst', { index, tekst });
}

/** Zet "Persoonsgegevens vervangen" aan of uit voor het verslag (V-2). */
export function vergaderingPersoonVervangen(aan: boolean): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_persoon_vervangen', { aan });
}

/** Haalt een markering weg (V-2): een gevonden stuk overal, een zelf gemarkeerd stuk helemaal. */
export function vergaderingPersoonWeghalen(waarde: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_persoon_weghalen', { waarde });
}

/** Markeert zelf een stuk tekst (V-2), overal waar het als los stuk voorkomt. */
export function vergaderingPersoonMarkeer(waarde: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_persoon_markeer', { waarde });
}

/**
 * Het stuk geluid van de opname tussen twee tijden (de tijdlijn van het verslag), beide sporen
 * gemengd, als WAV-bytes (V-1). De webview maakt er een blob van; ze krijgt nooit een pad.
 */
export function vergaderingAudioStuk(beginMs: number, eindMs: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('vergadering_audio_stuk', { beginMs, eindMs });
}

/**
 * Geeft een beurt aan een andere spreker (4b-D). Met `null` ("Nieuwe spreker") krijgt de beurt
 * het eerstvolgende vrije nummer (tot en met 8); ook een beurt zonder spreker kan zo een spreker
 * krijgen. De beurt gaat daarna samen met een buur van dezelfde spreker, dus de indexen in de
 * nieuwe stand kunnen anders zijn.
 */
export function vergaderingGeefAanSpreker(
  index: number,
  spreker: number | null
): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_geef_aan_spreker', { index, spreker });
}

/** Voegt spreker `van` samen met spreker `naar` (4b-D): al zijn beurten worden van `naar`. */
export function vergaderingVoegSprekersSamen(van: number, naar: number): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_voeg_sprekers_samen', { van, naar });
}

/**
 * Slaat het verslag op, zoals een transcript (4-C). Bij `bestaatAl` antwoordt de interface met
 * `bestandBewaarBeslis` uit bewaren.ts.
 */
export function vergaderingOpslaan(
  formaat: Formaat | null,
  anderePlek: boolean
): Promise<BewaarUitslag> {
  return invoke<BewaarUitslag>('vergadering_opslaan', { formaat, anderePlek });
}

/** Zet het verslag als tekst op het klembord. */
export function vergaderingKopieer(): Promise<void> {
  return invoke<undefined>('vergadering_kopieer');
}

/**
 * De standaardnamen van de sprekers in de taal van de app, zoals Rust ze nodig heeft voor Kopiëren
 * en Opslaan (`Sprekerlabels` in mondig-core): "Jij"/"You", "Anderen"/"Others" en per nummer
 * "Spreker 1"/"Speaker 1". Een naam die de gebruiker gaf, blijft zoals hij is.
 */
export interface Sprekerlabels {
  jij: string;
  anderen: string;
  /** Plek 0 is spreker 1, tot en met `MAX_SPREKERS`. */
  sprekers: string[];
}

/** Alle standaardnamen in de taal van `t`, voor `vergaderingZetLabels`. */
export function sprekerlabels(
  t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen' | 'vergaderSpreker'>
): Sprekerlabels {
  return {
    jij: t.sprekerJij,
    anderen: t.sprekerAnderen,
    sprekers: Array.from({ length: MAX_SPREKERS }, (_, plek) => t.vergaderSpreker(plek + 1))
  };
}

/** Geeft Rust de standaardnamen van de sprekers in de taal van de app, voor Kopiëren en Opslaan. */
export function vergaderingZetLabels(labels: Sprekerlabels): Promise<void> {
  return invoke<undefined>('vergadering_zet_labels', { labels });
}

/** Bewaart de opname als één WAV-bestand. Geeft de bestandsnaam, of `null` bij annuleren. */
export function vergaderingAudioBewaren(): Promise<string | null> {
  return invoke<string | null>('vergadering_audio_bewaren');
}

/**
 * Sluit het verslag. Er wordt niets gewist: het staat daarna onder "Eerder opgenomen". Is het
 * verslag niet bewaard (de schijf was vol), dan weigert Mondig met een melding; met `toch` sluit
 * het verslag dan wel.
 */
export function vergaderingSluiten(toch = false): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_sluiten', { toch });
}

/** Opent een bewaard verslag uit de lijst "Eerder opgenomen". */
export function vergaderingVerslagOpenen(id: string, toch = false): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_verslag_openen', { id, toch });
}

/** Wist een verslag met zijn opname: zonder `id` het verslag dat open is, anders een uit de lijst. */
export function vergaderingVerslagWissen(id: string | null): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_verslag_wissen', { id });
}

/** De modellen waarmee het open verslag opnieuw kan worden uitgeschreven (ADR 0023). */
export function vergaderingOpnieuwKeuzes(): Promise<OpnieuwKeuzes> {
  return invoke<OpnieuwKeuzes>('vergadering_opnieuw_keuzes');
}

/**
 * Begint het open verslag opnieuw uit te schrijven met dit model. Tijdens het lopen blijft het
 * verslag te lezen, te kopiëren en op te slaan; wijzigen en sluiten weigeren.
 */
export function vergaderingOpnieuwStart(model: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_opnieuw_start', { model });
}

/** Stopt het opnieuw uitschrijven: het verslag blijft zoals het was. */
export function vergaderingOpnieuwStoppen(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_opnieuw_stoppen');
}

/** Zet de versie van voor het opnieuw uitschrijven terug (kan tot het verslag sluit). */
export function vergaderingVorigeTerugzetten(): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_vorige_terugzetten');
}

/** Geeft het verslag dat open is een naam; een lege naam zet de titel terug die Mondig zelf maakt. */
export function vergaderingVerslagNaam(naam: string): Promise<VergaderingStand> {
  return invoke<VergaderingStand>('vergadering_verslag_naam', { naam });
}

// ---------------------------------------------------------------------------
// Pure hulpjes voor het scherm
// ---------------------------------------------------------------------------

/** Welk deel van het scherm er te zien is. */
export type Scherm = 'klaar' | 'starten' | 'voorbereiden' | 'opname' | 'uitschrijven' | 'verslag';

/** Het deel van het scherm bij een toestand van Rust. */
export function schermVan(toestand: StandToestand): Scherm {
  switch (toestand.type) {
    case 'klaar':
      return 'klaar';
    case 'starten':
      return 'starten';
    case 'voorbereiden':
      return 'voorbereiden';
    case 'neemtOp':
      return 'opname';
    case 'uitschrijven':
      return 'uitschrijven';
    case 'verslag':
      return 'verslag';
  }
}

/** De klok van de opname: "0:12" onder het uur, "1:02:05" vanaf een uur (zie duur.ts). */
export function klokTijd(ms: number): string {
  return formatDuur(ms);
}

/** De duur in hele minuten, minstens 1: "52 minuten" en nooit "0 minuten". */
export function duurMinuten(ms: number): number {
  return Math.max(1, Math.round(ms / 60_000));
}

/** Was een moment vandaag, gisteren of eerder? Op kalenderdagen in de eigen tijdzone. */
export function dagVan(ms: number, nu: number): 'vandaag' | 'gisteren' | 'eerder' {
  const begin = (moment: number): number => {
    const d = new Date(moment);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  };
  const dag = new Date(begin(nu));
  dag.setDate(dag.getDate() - 1);
  const gisteren = dag.getTime();
  const dagVanMoment = begin(ms);
  if (dagVanMoment === begin(nu)) return 'vandaag';
  if (dagVanMoment === gisteren) return 'gisteren';
  return 'eerder';
}

const DAG_MS = 86_400_000;

/**
 * Meldt vooraf dat een opname binnen 24 uur vanzelf wordt gewist: "vandaag" als dat nog op
 * dezelfde kalenderdag is (of al had gemoeten, Mondig ruimt elk uur op), anders "morgen".
 * Verder dan 24 uur weg: `null`, dan staat er niets bij.
 */
export function wistBinnenkort(wistMs: number | null, nu: number): 'vandaag' | 'morgen' | null {
  if (wistMs === null || wistMs - nu > DAG_MS) return null;
  return wistMs <= nu || dagVan(wistMs, nu) === 'vandaag' ? 'vandaag' : 'morgen';
}

/** De keuzes voor "Opnames en verslagen wissen na", in dagen. Dezelfde als in Rust. */
export const BEWAAR_DAGEN = [1, 7, 30] as const;
export type BewaarDagen = (typeof BEWAAR_DAGEN)[number];

/** De standaard: 7 dagen. */
export const STANDAARD_BEWAAR_DAGEN: BewaarDagen = 7;

/** Is dit een keuze die Rust kent? */
export function isBewaarDagen(waarde: number): waarde is BewaarDagen {
  return (BEWAAR_DAGEN as readonly number[]).includes(waarde);
}

/** De bewaarde bewaartermijn in dagen. */
export async function bewaartermijnInstelling(): Promise<BewaarDagen> {
  const dagen = await invoke<number>('bewaartermijn_instelling');
  return isBewaarDagen(dagen) ? dagen : STANDAARD_BEWAAR_DAGEN;
}

/** Bewaart een andere bewaartermijn. */
export function bewaartermijnZet(dagen: BewaarDagen): Promise<void> {
  return invoke<undefined>('bewaartermijn_zet', { dagen });
}

/**
 * Hoort bij deze soort een naam bij elke beurt? Online altijd (Jij en Anderen), en in de ruimte
 * alleen als de sprekers herkend zijn (4b-D). Alleen ik nooit. Zonder sprekernummers blijft het
 * verslag zoals het was.
 */
export function toontLabels(soort: Vergadersoort, beurten: readonly BeurtInfo[] = []): boolean {
  return soort === 'online' || (soort === 'ruimte' && heeftSprekers(beurten));
}

/** Zijn de sprekers herkend? Dan heeft minstens één beurt een sprekernummer. */
export function heeftSprekers(beurten: readonly Pick<BeurtInfo, 'sprekernummer'>[]): boolean {
  return beurten.some((b) => b.sprekernummer !== null);
}

/**
 * De kant waarvan de sprekers herkend worden: online "Anderen", in de ruimte de microfoon.
 * Alleen ik heeft geen sprekers. Dezelfde regel als `Gesprek::sprekerkant` in Rust.
 */
export function sprekerKant(soort: Vergadersoort): Kant | null {
  if (soort === 'online') return 'anderen';
  return soort === 'ruimte' ? 'jij' : null;
}

/** Hoeveel sprekers er hooguit zijn (Rust: `MAX_SPREKERS`). */
export const MAX_SPREKERS = 8;

/** Eén spreker in het verslag, voor de knoppen in de popover. */
export interface SprekerInfo {
  nummer: number;
  /** De naam die het verslag toont: een gegeven naam, anders "Spreker 1". */
  naam: string;
}

/** De sprekers in het verslag, oplopend op nummer; de eerste beurt van een nummer geeft de naam. */
export function sprekersVan(
  beurten: readonly BeurtInfo[],
  t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen' | 'vergaderSpreker'>
): SprekerInfo[] {
  const gezien = new Map<number, SprekerInfo>();
  for (const b of beurten) {
    if (b.sprekernummer !== null && !gezien.has(b.sprekernummer)) {
      gezien.set(b.sprekernummer, { nummer: b.sprekernummer, naam: beurtNaam(b, t) });
    }
  }
  return [...gezien.values()].sort((a, b) => a.nummer - b.nummer);
}

/**
 * Kan deze beurt aan een spreker gegeven worden? Alleen op de kant waarvan de sprekers herkend
 * worden: online heeft "Jij" geen sprekers. Online alleen in een verslag met herkende sprekers; in
 * de ruimte ook zonder, want daar maakt de gebruiker zelf de eerste spreker (Rust:
 * `Gesprek::geef_aan_nieuwe_spreker`). Zonder sprekers staat er geen labelkolom ([`toontLabels`]),
 * en bij aanwijzen verschijnt de knop "Spreker aanwijzen".
 */
export function kanAanSpreker(
  soort: Vergadersoort,
  beurten: readonly BeurtInfo[],
  beurt: Pick<BeurtInfo, 'kant'>
): boolean {
  return (soort === 'ruimte' || heeftSprekers(beurten)) && beurt.kant === sprekerKant(soort);
}

/**
 * Is dit een alinea in de ruimte die niemand aanwees? Zo'n alinea heeft geen spreker en geen
 * naam (Rust: `Gesprek::label` geeft `None`): de labelkolom toont "+ Spreker" in plaats van "Jij",
 * en zoeken, kopie, export en notulen noemen er geen naam bij.
 */
export function zonderSpreker(
  soort: Vergadersoort,
  beurt: Pick<BeurtInfo, 'sprekernummer' | 'eigenNaam'>
): boolean {
  return soort === 'ruimte' && beurt.sprekernummer === null && !beurt.eigenNaam;
}

/** Neemt deze soort ook het geluid van de computer op? Alleen online. */
export function metSysteemgeluid(soort: Vergadersoort): boolean {
  return soort === 'online';
}

/**
 * Komt er vóór de start de herinnering "Vertel het de anderen"? Bij elke vergadering wel
 * (ADR 0008), bij Alleen ik niet: daar is niemand om het aan te vertellen.
 */
export function vraagtHerinnering(soort: Vergadersoort): boolean {
  return soort !== 'alleen';
}

/**
 * Zoveel wachtende opnames staan er hooguit in beeld; de rest achter "Alle N tonen". De lijst
 * staat onder de startknop, dus een langere lijst duwt "Opname starten" niet uit beeld.
 */
export const WACHTEND_ZICHTBAAR = 10;

/**
 * De soort van een wachtende opname. Een onbekende soort telt als online, net als in Rust
 * (`lees_soort`): zo gedroeg elke opname zich voor er soorten waren.
 */
export function soortVan(opname: Pick<OpnameInfo, 'soort'>): Vergadersoort {
  return SOORTEN.includes(opname.soort) ? opname.soort : 'online';
}

/** De wachtende opnames van de soort die in de schakelaar staat. */
export function opnamesVanSoort(
  opnames: readonly OpnameInfo[],
  soort: Vergadersoort
): OpnameInfo[] {
  return opnames.filter((o) => soortVan(o) === soort);
}

/** Hoeveel opnames er nog wachten van een andere soort dan die in de schakelaar. */
export interface AndereSoort {
  soort: Vergadersoort;
  aantal: number;
  /**
   * De vroegste dag waarop er een vanzelf gewist wordt, als dat vandaag of morgen is; anders
   * `null`. Dan waarschuwt de regel, zodat geen opname onopgemerkt verdwijnt.
   */
  wist: 'vandaag' | 'morgen' | null;
  /** Hoeveel opnames er op die dag gewist worden (0 als `wist` `null` is). */
  aantalWist: number;
}

/**
 * Per andere soort met wachtende opnames: hoeveel, en of er binnenkort een gewist wordt. In de
 * volgorde van de schakelaar; soorten zonder opnames vallen weg.
 */
export function andereSoorten(
  opnames: readonly OpnameInfo[],
  soort: Vergadersoort,
  nu: number
): AndereSoort[] {
  return SOORTEN.filter((s) => s !== soort).flatMap((s) => {
    const van = opnamesVanSoort(opnames, s);
    if (van.length === 0) return [];
    const dagen = van.map((o) => wistBinnenkort(o.wistMs, nu));
    const wist = dagen.includes('vandaag') ? 'vandaag' : dagen.includes('morgen') ? 'morgen' : null;
    const aantalWist = wist === null ? 0 : dagen.filter((d) => d === wist).length;
    return [{ soort: s, aantal: van.length, wist, aantalWist }];
  });
}

/**
 * De wachtende opnames die in beeld staan, nieuwste eerst (zoals in het ontwerp): alle, of de
 * nieuwste WACHTEND_ZICHTBAAR. Rust geeft ze oudste eerst.
 */
export function zichtbareOpnames(opnames: readonly OpnameInfo[], alles: boolean): OpnameInfo[] {
  const nieuwsteEerst = [...opnames].sort((a, b) => b.beginMs - a.beginMs);
  return alles ? nieuwsteEerst : nieuwsteEerst.slice(0, WACHTEND_ZICHTBAAR);
}

/** Zoveel bewaarde verslagen staan er hooguit in beeld; de rest achter "Alle N tonen". */
export const VERSLAGEN_ZICHTBAAR = 5;

/** De bewaarde verslagen die in beeld staan, nieuwste eerst: alle, of de nieuwste vijf. */
export function zichtbareVerslagen(
  verslagen: readonly VerslagInfo[],
  alles: boolean
): VerslagInfo[] {
  const nieuwsteEerst = [...verslagen].sort((a, b) => b.beginMs - a.beginMs);
  return alles ? nieuwsteEerst : nieuwsteEerst.slice(0, VERSLAGEN_ZICHTBAAR);
}

/** Tot wanneer Mondig een verslag bewaart, als dag voor de zin "Mondig bewaart dit verslag tot ...". */
export type BewaarTot =
  | { soort: 'vandaag' }
  | { soort: 'morgen' }
  | { soort: 'weekdag'; ms: number }
  | { soort: 'datum'; ms: number };

/**
 * De dag waarop Mondig een verslag wist, in woorden: vandaag, morgen, een weekdag als dat binnen
 * een week is ("dinsdag"), anders een datum. `null` als Mondig de bewaartermijn niet kent.
 */
export function bewaarTot(wistMs: number | null, nu: number): BewaarTot | null {
  if (wistMs === null) return null;
  const begin = (moment: number): number => {
    const d = new Date(moment);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  };
  const dagen = Math.round((begin(wistMs) - begin(nu)) / DAG_MS);
  if (dagen <= 0) return { soort: 'vandaag' };
  if (dagen === 1) return { soort: 'morgen' };
  return dagen < 7 ? { soort: 'weekdag', ms: wistMs } : { soort: 'datum', ms: wistMs };
}

/** De standaardnaam van een kant: "Jij" of "Anderen". */
export function kantNaam(kant: Kant, t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen'>): string {
  return kant === 'jij' ? t.sprekerJij : t.sprekerAnderen;
}

/**
 * Wat er op het label van een beurt staat: de naam die de gebruiker gaf, anders "Spreker 1" (als
 * de sprekers herkend zijn), anders "Jij" of "Anderen" in de taal van de app (Rust geeft die
 * standaardnamen altijd in het Nederlands).
 */
export function beurtNaam(
  beurt: Pick<BeurtInfo, 'kant' | 'spreker' | 'eigenNaam'> &
    Partial<Pick<BeurtInfo, 'sprekernummer'>>,
  t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen' | 'vergaderSpreker'>
): string {
  if (beurt.eigenNaam && beurt.spreker !== null && beurt.spreker !== '') return beurt.spreker;
  if (beurt.sprekernummer != null) return t.vergaderSpreker(beurt.sprekernummer);
  return kantNaam(beurt.kant, t);
}

/**
 * Wat er op het label van een beurt staat: zoals `beurtNaam`, maar een eigen naam met een gemarkeerd
 * gegeven erin staat er vervangen (`[GEGEVEN]`) als "Persoonsgegevens vervangen" aan staat, zoals
 * Kopiëren en Opslaan hem geven.
 */
export function beurtNaamGetoond(
  beurt: Pick<BeurtInfo, 'kant' | 'spreker' | 'sprekerGetoond' | 'eigenNaam'> &
    Partial<Pick<BeurtInfo, 'sprekernummer'>>,
  t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen' | 'vergaderSpreker'>
): string {
  return beurt.eigenNaam && beurt.sprekerGetoond != null && beurt.sprekerGetoond !== ''
    ? beurt.sprekerGetoond
    : beurtNaam(beurt, t);
}

/**
 * De naam van een beurt zonder eigen naam: "Spreker 2" als de sprekers herkend zijn, anders "Jij"
 * of "Anderen". Wat er na leegmaken staat.
 */
export function standaardNaam(
  beurt: Pick<BeurtInfo, 'kant' | 'sprekernummer'>,
  t: Pick<Berichten, 'sprekerJij' | 'sprekerAnderen' | 'vergaderSpreker'>
): string {
  return beurt.sprekernummer !== null
    ? t.vergaderSpreker(beurt.sprekernummer)
    : kantNaam(beurt.kant, t);
}

/**
 * Wat de melding (en de schermlezer) zegt na een naam, uit wat Rust teruggaf: "Spreker 1 en
 * Spreker 2 zijn nu Noor" als er sprekers samengingen, "Terug naar Spreker 2" na leegmaken (de naam
 * die er dan echt staat), anders "Naam gegeven: Noor", met de naam zoals hij in het verslag staat.
 */
export function naamMelding(
  stap: Pick<Sprekerstap, 'kant' | 'samengevoegd' | 'naam' | 'nummer'>,
  t: Pick<
    Berichten,
    | 'sprekerJij'
    | 'sprekerAnderen'
    | 'vergaderSpreker'
    | 'vergaderNaamTerug'
    | 'vergaderNaamGegeven'
    | 'vergaderZijnNu'
  >
): string {
  const standaard = standaardNaam({ kant: stap.kant, sprekernummer: stap.nummer }, t);
  if (stap.samengevoegd.length >= 2) {
    const namen = stap.samengevoegd.map((nummer) =>
      standaardNaam({ kant: stap.kant, sprekernummer: nummer }, t)
    );
    return t.vergaderZijnNu(namen, stap.naam ?? standaard);
  }
  if (stap.naam === null) return t.vergaderNaamTerug(standaard);
  return t.vergaderNaamGegeven(stap.naam);
}

/** De naam waarmee het invoerveld begint: de eigen naam, of leeg bij de standaardnaam. */
export function beginNaam(beurt: Pick<BeurtInfo, 'spreker' | 'eigenNaam'>): string {
  return beurt.eigenNaam && beurt.spreker !== null ? beurt.spreker : '';
}

/** Een gebeurtenis die de schermlezer meldt, afgeleid uit twee opeenvolgende standen. */
export type Aankondiging = 'gestart' | 'uitschrijven' | 'verslag';

/**
 * Wat er sinds de vorige stand veranderde dat de schermlezer moet melden: het begin van de
 * opname, het begin van het uitschrijven of het verslag. Een fout meldt het scherm zelf
 * (een foutvak met `role="alert"`). `null` als er niets nieuws is, of als dit de eerste stand
 * is: wie het scherm opent terwijl er al een opname loopt, hoort geen "gestart".
 */
export function aankondiging(
  vorig: VergaderingStand | null,
  nu: VergaderingStand
): Aankondiging | null {
  if (vorig === null) return null;
  const was = vorig.toestand.type;
  const is = nu.toestand.type;
  if (is === was) return null;
  if (is === 'verslag') return 'verslag';
  if (is === 'neemtOp') return 'gestart';
  if (is === 'uitschrijven') return 'uitschrijven';
  return null;
}

const GEEN_EVENTS: Promise<UnlistenFn> = Promise.resolve(() => undefined);

/** Elke nieuwe stand van het scherm. */
export function luisterNaarStand(bij: (stand: VergaderingStand) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<VergaderingStand>('vergadering-gewijzigd', (event) => {
    bij(event.payload);
  });
}

/** Het systeemvak vraagt om een nieuwe opname (met de vraag vooraf); haal het verzoek dan op. */
export function luisterNaarStartGevraagd(bij: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen('vergadering-start-gevraagd', () => {
    bij();
  });
}

/** Het niveau per spoor tijdens de opname, ongeveer tien keer per seconde. */
export function luisterNaarNiveau(bij: (niveaus: Niveaus) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<Niveaus>('vergadering-niveau', (event) => {
    bij(event.payload);
  });
}
