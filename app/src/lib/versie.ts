import { invoke } from '@tauri-apps/api/core';
import { versieRegelApparaat, type Rekenstand } from './videokaart';

// Dunne laag om de commando's van het scherm Over Mondig, plus de kleine pure regels erachter.

/** Het actieve model, voor de te kopiëren versiegegevens. */
export interface ActiefModel {
  id: string;
  naam: string;
}

/** Wat de app over zijn eigen uitgave weet. Geen persoonsgegevens. */
export interface VersieInfo {
  versie: string;
  /** Korte commit, of `onbekend`. */
  bouw: string;
  /** `jjjj-mm-dd`. */
  datum: string;
  systeem: string;
  processor: string;
  model: ActiefModel;
}

/** De naamsvermelding van één model (uit modellen.json). */
export interface ModelVermelding {
  id: string;
  naam: string;
  hulpmodel: boolean;
  maker: string;
  titel: string;
  copyright: string | null;
  bronUrl: string;
  ggufUrl: string;
  licentie: string;
  licentieUrl: string;
  disclaimerUrl: string;
  wijziging: string;
}

export function haalVersieInfo(): Promise<VersieInfo> {
  return invoke<VersieInfo>('versie_info');
}

/**
 * Bouwcode en protocolversie van het laatst gestarte werkproces, of `null` als dat nog niet
 * gestart is. Niet in `VersieInfo`: die wordt één keer opgehaald, het werkproces start later.
 */
export function haalWerkprocesBouw(): Promise<string | null> {
  return invoke<string | null>('werkproces_bouw');
}

export function haalModelVermeldingen(): Promise<ModelVermelding[]> {
  return invoke<ModelVermelding[]>('model_vermeldingen');
}

/** De versie waarvoor "Wat is er nieuw" al getoond is, of `null` bij een eerste start. */
export function laatstGezienVersie(): Promise<string | null> {
  return invoke<string | null>('laatst_geziene_versie');
}

/** Onthoudt dat "Wat is er nieuw" voor deze versie gesloten is. */
export function zetLaatstGezienVersie(versie: string): Promise<void> {
  return invoke<undefined>('laatst_geziene_versie_zet', { versie });
}

const MAANDEN = [
  'jan',
  'feb',
  'mrt',
  'apr',
  'mei',
  'jun',
  'jul',
  'aug',
  'sep',
  'okt',
  'nov',
  'dec'
];

/** `2026-11-01` wordt `1 nov 2026`. Onleesbare invoer komt ongewijzigd terug. */
export function korteDatum(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  const maand = MAANDEN[Number(m?.[2]) - 1];
  if (!m || !maand) return iso;
  return `${String(Number(m[3]))} ${maand} ${m[1] ?? ''}`;
}

/**
 * De tekst van "Versiegegevens kopiëren": voor een supportvraag, zonder persoonsgegevens
 * (geen gebruikersnaam, geen paden, geen computernaam). `werkproces` is wat het werkproces
 * over zijn bouw meldde; wijkt dat af van de bouw van de app, dan is de installatie niet
 * compleet.
 */
export function versieGegevensTekst(
  info: VersieInfo,
  werkproces: string | null,
  apparaat: Rekenstand | null = null
): string {
  return [
    `Mondig ${info.versie}`,
    `Bouw: ${info.bouw}, ${info.datum}`,
    `Werkproces: ${werkproces ?? 'nog niet gestart'}`,
    `Systeem: ${info.systeem}, ${info.processor}`,
    `Model: ${info.model.naam} (${info.model.id})`,
    // Waarop de motoren nu rekenen: processor, videokaart of terugval na een fout.
    versieRegelApparaat(apparaat)
  ].join('\n');
}

/** Opent een adres uit de naamsvermelding in de standaardbrowser (Mondig kent alleen die adressen). */
export function openVermeldingsadres(adres: string): Promise<void> {
  return invoke<undefined>('open_vermeldingsadres', { adres });
}

/**
 * Wat de kaart "Wat is er nieuw" doet bij het starten: `toon` na een update, `bewaar` bij een
 * eerste installatie (geen bewaarde versie: de huidige bewaren en niets tonen), `niets` als
 * deze versie al gezien is.
 */
export function kaartBesluit(gezien: string | null, huidig: string): 'toon' | 'bewaar' | 'niets' {
  if (gezien === null) return 'bewaar';
  return gezien === huidig ? 'niets' : 'toon';
}

/** De versie zoals de gebruiker hem leest: zonder `-dev` of bouwcode (`0.3.0-dev+abc` wordt `0.3.0`). */
export function versieZonderAchtervoegsel(versie: string): string {
  // Alleen de ontwikkelversie (-dev) en de bouwcode (+...) gaan eraf; een testversie als
  // 1.0.0-rc.1 blijft herkenbaar.
  return versie.replace(/\+.*$/, '').replace(/-dev$/, '');
}
