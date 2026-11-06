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
export type StripToestand = 'luistert' | 'verwerkt' | 'klaar' | 'fout';

/**
 * Welke toestand de strook toont, of `null` als hij weg is. Een lopend dictaat (luistert,
 * verwerkt) staat er zolang het loopt. "Klaar" en een fout staan er tot de gebruiker het
 * startscherm heeft gezien, want daar staan de tekst en de vervolgactie. Geannuleerd en
 * rust tonen niets.
 */
export function stripToestand(weergave: Weergave, gezien: boolean): StripToestand | null {
  switch (weergave.type) {
    case 'luistert':
    case 'verwerkt':
      return weergave.type;
    case 'klaar':
    case 'fout':
      return gezien ? null : weergave.type;
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
  | 'op_klembord'
  | 'beheerrechten'
  | 'toetsen_ingedrukt'
  | 'geen_venster'
  | 'mislukt'
  | 'in_mondig';

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
      return 'gelukt';
    case 'waarschijnlijk_geplakt':
      // Geen fout: misschien plakte het wel. Een status zonder stellige uitspraak.
      return 'klembord';
    case 'op_klembord':
    case 'in_mondig':
      return 'klembord';
    case 'niet_gelezen':
    case 'niet_geplakt':
    case 'beheerrechten':
    case 'toetsen_ingedrukt':
    case 'geen_venster':
    case 'mislukt':
      return 'fout';
  }
}
