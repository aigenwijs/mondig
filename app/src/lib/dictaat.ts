import type { Gebeurtenis, ProefResultaat, Vervolg } from './taak';

// De toestand van een dictaat met de sneltoets, zoals het startscherm die toont. Alle
// beslissingen zitten in deze pure functies, zodat ze zonder venster te testen zijn.

/** Na negen minuten waarschuwt het scherm dat de opname bijna vol is (ADR 0012). */
export const WAARSCHUWING_NA_MS = 9 * 60 * 1000;

/** Wat het startscherm van het huidige dictaat toont. */
export type Weergave =
  | { type: 'rust' }
  | { type: 'luistert'; taak: number; sinds: number }
  | { type: 'verwerkt'; taak: number }
  | { type: 'klaar'; taak: number; uitslag: ProefResultaat }
  | { type: 'fout'; taak: number; melding: string; vervolg: Vervolg }
  | { type: 'geannuleerd'; taak: number };

export const RUST: Weergave = { type: 'rust' };

/** Loopt het dictaat nog (luistert of verwerkt)? Dan kan de gebruiker annuleren. */
export function isActief(weergave: Weergave): boolean {
  return weergave.type === 'luistert' || weergave.type === 'verwerkt';
}

/**
 * Verwerkt een `taak`-event. Alleen dictaten tellen mee. Een lopend dictaat volgt alleen
 * zijn eigen gebeurtenissen; is er niets actief, dan begint een nieuwe taak met "luistert"
 * of, als het dictaat al mislukt voordat het begon (geen model), met een fout.
 * "Klaar" wacht op de uitslag zelf (`dictaat-resultaat`), want die bevat de tekst.
 */
export function opGebeurtenis(huidig: Weergave, gebeurtenis: Gebeurtenis, nu: number): Weergave {
  if (gebeurtenis.soort !== 'dictaat') return huidig;
  const { taak, toestand } = gebeurtenis;
  if ((huidig.type === 'luistert' || huidig.type === 'verwerkt') && huidig.taak !== taak) {
    return huidig;
  }
  switch (toestand.type) {
    case 'luistert':
      return { type: 'luistert', taak, sinds: nu };
    case 'verwerkt':
      return { type: 'verwerkt', taak };
    case 'fout':
      return { type: 'fout', taak, melding: toestand.melding, vervolg: toestand.vervolg };
    case 'geannuleerd':
      return { type: 'geannuleerd', taak };
    case 'wacht':
    case 'klaar':
      return huidig;
  }
}

/** Verwerkt de uitslag van een dictaat dat uitgeschreven werd. */
export function opUitslag(huidig: Weergave, uitslag: ProefResultaat): Weergave {
  if (huidig.type !== 'verwerkt' || huidig.taak !== uitslag.taak) return huidig;
  return { type: 'klaar', taak: uitslag.taak, uitslag };
}

/** Is de opname negen minuten of langer bezig? */
export function bijnaVol(sinds: number, nu: number): boolean {
  return nu - sinds >= WAARSCHUWING_NA_MS;
}

export { formatDuur } from './duur';

/** De kloktijd van een moment, in de taal van de app (`14:02`), voor bij "Laatste dictaat". */
export function klokTijd(ms: number, taal: string): string {
  return new Intl.DateTimeFormat(taal, { hour: '2-digit', minute: '2-digit' }).format(new Date(ms));
}

/** Wat de strook bovenaan de andere schermen toont. */
export type StripToestand = 'plakprobleem' | 'fout';

/**
 * Welke toestand de strook toont, of `null` als hij weg is. Tijdens luisteren en verwerken
 * blijft hij weg: de dicteerbalk toont dat al. Hij verschijnt alleen als er na afloop iets
 * mis is en de gebruiker het nog niet zag: een fout, of een tekst die niet is geplakt
 * (`plakSoort` is niet "gelukt"). "Gezien" is zodra het startscherm open was, want daar
 * staan de tekst en de vervolgactie. Een dictaat dat gewoon geplakt is, toont niets.
 */
export function stripToestand(
  weergave: Weergave,
  gezien: boolean,
  plak: PlakUitkomst | null
): StripToestand | null {
  if (gezien) return null;
  switch (weergave.type) {
    case 'fout':
      return 'fout';
    case 'klaar':
      // "Waarschijnlijk geplakt" is geen probleem: meestal plakte het wel, en een waarschuwing
      // na elk zo'n dictaat zou vals alarm zijn. Dicteren toont er wel een rustige status bij.
      if (plak === null || plak === 'waarschijnlijk_geplakt') return null;
      return plakSoort(plak) !== 'gelukt' ? 'plakprobleem' : null;
    case 'luistert':
    case 'verwerkt':
    case 'rust':
    case 'geannuleerd':
      return null;
  }
}

/** Hoe het plakken afliep (zelfde namen als `PlakUitkomst` in Rust). */
export type PlakUitkomst =
  | 'geplakt'
  | 'waarschijnlijk_geplakt'
  | 'niet_gelezen'
  | 'niet_geplakt'
  /** Het programma las het klembord niet, dus Mondig typte de tekst. */
  | 'getypt'
  /** Mondig typte een deel en stopte (ander venster, een toets, beveiligde invoer); de rest staat op het klembord. */
  | 'typen_onderbroken'
  /** Ook typen lukte niet; de tekst (of de rest) staat op het klembord. */
  | 'typen_mislukt'
  /** Niets gelezen en geen tekstveld met de focus: niet getypt, de tekst staat op het klembord. */
  | 'geen_tekstveld'
  | 'op_klembord'
  | 'beheerrechten'
  | 'toetsen_ingedrukt'
  | 'geen_venster'
  | 'mislukt'
  | 'in_mondig'
  /** Alleen op de Mac: geen recht op toegankelijkheid; de tekst staat op het klembord. */
  | 'geen_toegang'
  /** Alleen op de Mac: niets gelezen terwijl beveiligde invoer aanstond; de tekst staat op het klembord. */
  | 'wachtwoordveld';

/** Wat er met het klembord van voor het plakken gebeurde (zelfde namen als in Rust). */
export type VorigKlembord = 'bewaard' | 'onvolledig' | 'kwijt';

/**
 * Inhoud van het event `plak-uitslag`. Kan voor dezelfde taak een tweede keer komen, als
 * het terugzetten pas later definitief mislukt.
 */
export interface PlakUitslag {
  taak: number;
  uitkomst: PlakUitkomst;
  /** Wat er met het klembord van voor het plakken gebeurde. */
  vorigKlembord: VorigKlembord;
}

/**
 * Hoort deze plakuitslag bij het dictaat in beeld? Alleen bij een klaar dictaat met
 * hetzelfde taak-id; een late uitslag van een vorig dictaat telt niet meer.
 */
export function plakHoortBij(weergave: Weergave, uitslag: PlakUitslag): boolean {
  return weergave.type === 'klaar' && weergave.taak === uitslag.taak;
}

/**
 * Hoe de interface een uitkomst meldt: gelukt als rustige status, een terugval die werkte
 * als status, en alles waarbij de tekst niet waar je typt staat als fout (`alert`) met
 * Kopiëren als vervolgactie (ADR 0012, "Tekst nooit kwijt").
 */
export function plakSoort(uitkomst: PlakUitkomst): 'gelukt' | 'klembord' | 'fout' {
  switch (uitkomst) {
    case 'geplakt':
    case 'getypt':
      return 'gelukt';
    case 'waarschijnlijk_geplakt':
      // Geen fout: misschien plakte het wel. Een status zonder stellige uitspraak.
      return 'klembord';
    case 'op_klembord':
    case 'in_mondig':
    case 'wachtwoordveld':
      return 'klembord';
    case 'niet_gelezen':
    case 'niet_geplakt':
    case 'beheerrechten':
    case 'toetsen_ingedrukt':
    case 'geen_venster':
    case 'mislukt':
    case 'typen_onderbroken':
    case 'typen_mislukt':
    case 'geen_tekstveld':
    case 'geen_toegang':
      // Ook `geen_toegang` (Mac) is een fout, al staat de tekst op het klembord: zonder het
      // recht plakt Mondig nooit, en de melding wijst de weg naar de instelling.
      return 'fout';
  }
}
