import { invoke } from '@tauri-apps/api/core';
import type { Berichten } from './i18n/typen';
import type { BestandenStand } from './bestanden';

// Bewaren en exporteren (sprint 4-C): de commando's uit
// app/src-tauri/src/bewaren.rs en de pure hulpjes voor het scherm. De interface geeft nooit een
// pad: de map komt uit de instelling of de mapkiezer (in Rust), de naam uit de bouwstenen.

/** Een formaat; de sleutel is de extensie. */
export type Formaat = 'docx' | 'pdf' | 'odt' | 'txt' | 'md' | 'html' | 'srt' | 'vtt';

/** Alle formaten, in de volgorde van het menu. */
export const FORMATEN: readonly Formaat[] = [
  'docx',
  'pdf',
  'odt',
  'txt',
  'md',
  'html',
  'srt',
  'vtt'
];

/** Een onderdeel van de bestandsnaam. */
export type Bouwsteen = 'datum' | 'naam' | 'label' | 'tijd' | 'model';

/** Alle bouwstenen, in de volgorde van het ontwerp. */
export const BOUWSTENEN: readonly Bouwsteen[] = ['datum', 'naam', 'label', 'tijd', 'model'];

/** Een map zoals Rust hem geeft. */
export interface MapInfo {
  pad: string;
  /** Ligt de map in Documenten, dan het deel daarna ("Mondig"), anders `null`. */
  inDocumenten: string | null;
}

/** De keuzes in de groep Bestanden van Instellingen. */
export interface BewaarInstellingen {
  map: MapInfo | null;
  bouwstenen: Bouwsteen[];
  formaat: Formaat;
  tijden: boolean;
  automatisch: boolean;
  /** De opname ook als WAV naast een vergaderverslag (standaard uit). */
  audio: boolean;
  /** De naam die de bouwstenen nu geven, voor het interview uit het ontwerp. */
  voorbeeld: string;
}

/** Hoe opslaan afliep. */
export type BewaarUitslag =
  | {
      type: 'opgeslagen';
      naam: string;
      map: MapInfo;
      /** Bij een vergaderverslag met "Opname ook bewaren als geluidsbestand": de WAV ernaast. */
      audio?: string;
      /** Waarom de WAV niet lukte; het tekstbestand staat er dan wel. */
      audioFout?: string;
    }
  | { type: 'bestaatAl'; vraag: number; naam: string; voorstel: string }
  | { type: 'geannuleerd' };

/** Het antwoord op "Er staat al een bestand met deze naam". */
export type Keuze = 'nummer' | 'vervangen' | 'annuleren';

/** Een bestand dat Mondig van een transcript opsloeg, voor de bevestiging van Verwijderen. */
export interface OpgeslagenInfo {
  naam: string;
  map: MapInfo;
}

/** Hoe Verwijderen afliep. */
export interface VerwijderUitslag {
  stand: BestandenStand;
  /** Is het transcript weg? Niet als een bestand open was of niet weg kon. */
  weg: boolean;
  verwijderd: string[];
  /** Gewijzigd of verplaatst na het opslaan: blijft staan. */
  gebleven: string[];
  /** Open in een ander programma: blijft staan, het transcript ook. */
  inGebruik: string[];
  /** Kon om een andere reden niet weg: blijft staan, het transcript ook. */
  mislukt: string[];
}

export function bewarenInstellingen(): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_instellingen');
}

/** Opent de mapkiezer (in Rust) en bewaart de gekozen map. */
export function bewarenKiesMap(): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_kies_map');
}

/** Opent de opslagmap in de Verkenner; Rust maakt hem aan als hij er nog niet is. */
export function bewarenOpenMap(): Promise<void> {
  return invoke<undefined>('bewaren_open_map');
}

export function bewarenZetBouwstenen(bouwstenen: Bouwsteen[]): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_zet_bouwstenen', { bouwstenen });
}

export function bewarenZetFormaat(formaat: Formaat): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_zet_formaat', { formaat });
}

export function bewarenZetTijden(aan: boolean): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_zet_tijden', { aan });
}

export function bewarenZetAutomatisch(aan: boolean): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_zet_automatisch', { aan });
}

/** Zet "Opname ook bewaren als geluidsbestand" aan of uit. */
export function bewarenZetAudio(aan: boolean): Promise<BewaarInstellingen> {
  return invoke<BewaarInstellingen>('bewaren_zet_audio', { aan });
}

/**
 * Slaat een transcript op. Zonder formaat het standaardformaat; met `anderePlek` opent eerst
 * de mapkiezer.
 */
export function bestandBewaar(
  taak: number,
  formaat: Formaat | null,
  anderePlek: boolean
): Promise<BewaarUitslag> {
  return invoke<BewaarUitslag>('bestand_bewaar', { taak, formaat, anderePlek });
}

export function bestandBewaarBeslis(vraag: number, keuze: Keuze): Promise<BewaarUitslag> {
  return invoke<BewaarUitslag>('bestand_bewaar_beslis', { vraag, keuze });
}

export function bestandZetLabel(taak: number, label: string): Promise<void> {
  return invoke<undefined>('bestand_zet_label', { taak, label });
}

/** Opent het bestand dat Mondig net opsloeg. */
export function bestandOpenOpgeslagen(taak: number): Promise<void> {
  return invoke<undefined>('bestand_open_opgeslagen', { taak });
}

/** Toont het bestand dat Mondig net opsloeg in de Verkenner. */
export function bestandToonOpgeslagen(taak: number): Promise<void> {
  return invoke<undefined>('bestand_toon_opgeslagen', { taak });
}

/** De bestanden die Verwijderen nu zou weghalen: dezelfde lijst die Rust daarna gebruikt. */
export function bestandOpgeslagenLijst(taak: number): Promise<OpgeslagenInfo[]> {
  return invoke<OpgeslagenInfo[]>('bestand_opgeslagen_lijst', { taak });
}

/** Verwijdert de bestanden die Mondig opsloeg (als ze ongewijzigd zijn) en het transcript. */
export function bestandVerwijder(taak: number): Promise<VerwijderUitslag> {
  return invoke<VerwijderUitslag>('bestand_verwijder', { taak });
}

/** De naam van een formaat zoals een leek hem kent: "Word", "PDF voor archief". */
export function formaatNaam(formaat: Formaat, t: Berichten): string {
  switch (formaat) {
    case 'docx':
      return t.formaatDocx;
    case 'pdf':
      return t.formaatPdf;
    case 'odt':
      return t.formaatOdt;
    case 'txt':
      return t.formaatTxt;
    case 'md':
      return t.formaatMd;
    case 'html':
      return t.formaatHtml;
    case 'srt':
      return t.formaatSrt;
    case 'vtt':
      return t.formaatVtt;
  }
}

/** De extensie klein achter de naam in het menu. */
export function formaatExtensie(formaat: Formaat): string {
  return formaat === 'pdf' ? '.pdf (PDF/A)' : `.${formaat}`;
}

/** De naam van een bouwsteen. */
export function bouwsteenNaam(steen: Bouwsteen, t: Berichten): string {
  switch (steen) {
    case 'datum':
      return t.bouwsteenDatum;
    case 'naam':
      return t.bouwsteenNaam;
    case 'label':
      return t.bouwsteenLabel;
    case 'tijd':
      return t.bouwsteenTijd;
    case 'model':
      return t.bouwsteenModel;
  }
}

/** Klik op een bouwsteen: staat hij aan, dan gaat hij eruit; anders komt hij achteraan. */
export function wisselBouwsteen(gekozen: readonly Bouwsteen[], steen: Bouwsteen): Bouwsteen[] {
  return gekozen.includes(steen) ? gekozen.filter((s) => s !== steen) : [...gekozen, steen];
}

/**
 * Zet een gekozen bouwsteen op een andere plek (`naarIndex`, vanaf 0, binnen de gekozen
 * bouwstenen). Staat de steen er niet bij, of is de plek al zijn plek, dan blijft de lijst
 * gelijk. De volgorde in deze lijst is ook de volgorde in de bestandsnaam (Rust gebruikt hem
 * zoals hij bewaard is).
 */
export function verplaatsBouwsteen(
  gekozen: readonly Bouwsteen[],
  steen: Bouwsteen,
  naarIndex: number
): Bouwsteen[] {
  const van = gekozen.indexOf(steen);
  if (van === -1) return [...gekozen];
  const naar = Math.min(gekozen.length - 1, Math.max(0, Math.trunc(naarIndex)));
  const rest = gekozen.filter((s) => s !== steen);
  rest.splice(naar, 0, steen);
  return rest;
}

/** Schuift een gekozen bouwsteen één plek: -1 naar voren (links), 1 naar achteren (rechts). */
export function schuifBouwsteen(
  gekozen: readonly Bouwsteen[],
  steen: Bouwsteen,
  richting: -1 | 1
): Bouwsteen[] {
  const van = gekozen.indexOf(steen);
  if (van === -1) return [...gekozen];
  return verplaatsBouwsteen(gekozen, steen, van + richting);
}

/** De plek van een gekozen bouwsteen, geteld vanaf 1 (voor de aankondiging); `null` als hij uit staat. */
export function plekVanBouwsteen(gekozen: readonly Bouwsteen[], steen: Bouwsteen): number | null {
  const plek = gekozen.indexOf(steen);
  return plek === -1 ? null : plek + 1;
}

/** De volgorde van de blokjes: eerst de gekozen in hun volgorde, dan de rest. */
export function steenVolgorde(gekozen: readonly Bouwsteen[]): Bouwsteen[] {
  return [...gekozen, ...BOUWSTENEN.filter((s) => !gekozen.includes(s))];
}

/** De map zoals de gebruiker hem kent: "Documenten\Mondig", anders het volledige pad. */
export function mapTekst(map: MapInfo | null, t: Berichten): string {
  if (map === null) return t.mapGeen;
  if (map.inDocumenten === null) return map.pad;
  if (map.inDocumenten === '') return t.mapDocumenten;
  const scheiding = map.pad.includes('\\') ? '\\' : '/';
  return `${t.mapDocumenten}${scheiding}${map.inDocumenten}`;
}

/** Het volgnummer uit een voorgestelde naam: "Naam (2).docx" geeft "(2)". */
export function volgnummer(voorstel: string): string {
  const gevonden = /\((\d+)\)(\.[^.]*)?$/.exec(voorstel);
  return gevonden ? `(${gevonden[1] ?? ''})` : '';
}
