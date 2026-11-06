import { WAARSCHUWING_NA_MS, plakSoort, type PlakUitkomst, type Weergave } from './dictaat';
import type { Label, PilOpschonen } from './opschonen';
import type { Uitkomst, Vervolg } from './taak';

// Wat de zwevende pil toont (sprint 3d). Alle beslissingen
// zitten in deze pure functies, zodat ze zonder venster te testen zijn. De pil leest dezelfde
// stand als het hoofdvenster (dictaatstand), plus het taak-id van het dictaat van de sneltoets.

/**
 * "Verwerkt" verschijnt pas als het uitschrijven langer dan een seconde duurt (ADR 0012).
 * Tot dan blijft de pil staan in een rustige tussenstand ("afronden": de meter stil, geen
 * draaiend teken), zodat hij niet verdwijnt en meteen weer verschijnt.
 */
export const VERWERKT_NA_MS = 1000;
/** "Geplakt" staat zo lang in beeld. */
export const KLAAR_ZICHTBAAR_MS = 1500;
/** Een melding zonder fout (op het klembord, geen tekst herkend) blijft wat langer staan. */
export const MELDING_ZICHTBAAR_MS = 4000;
/** Na tien minuten stopt de opname zelf (ADR 0012). */
export const MAX_OPNAME_MS = 10 * 60 * 1000;

/** Een melding zonder fout na het dictaat. */
export type KlaarSoort =
  'geplakt' | 'waarschijnlijk' | 'klembord' | 'in_mondig' | Exclude<Uitkomst, 'tekst'>;

/** Wat de pil toont. */
export type PilToestand =
  | { type: 'verborgen' }
  /** `restMs` is er pas na negen minuten: dan de waarschuwing. */
  | { type: 'luistert'; duurMs: number; restMs: number | null }
  /** Net losgelaten, nog geen seconde bezig: de meter stil, zonder draaiend teken. */
  | { type: 'afronden' }
  | { type: 'verwerkt' }
  /** Het dictaat gaat naar het taalmodel om op te schonen (ADR 0027), met het label erbij. */
  | { type: 'opschonen'; label: Label }
  | { type: 'klaar'; soort: KlaarSoort; duurMs: number }
  /**
   * Opschonen lukte niet (te laat, afgekeurd, gestopt): de letterlijke tekst is geplakt, of staat
   * op het klembord (`soort` zegt hoe het plakken afliep).
   */
  | { type: 'terugval'; soort: KlaarSoort }
  | { type: 'fout'; melding: string; vervolg: Vervolg }
  | { type: 'plakfout'; uitkomst: PlakUitkomst };

/** De momenten die de pil bijhoudt naast de stand zelf. */
export interface PilTijden {
  /** Het taak-id waarvoor de tijden gelden. */
  taak: number | null;
  /** Wanneer het uitschrijven begon. */
  verwerktSinds: number | null;
  /** Wanneer de afloop bekend werd (plakuitslag, of een uitslag zonder tekst). */
  afgerondSinds: number | null;
}

export const GEEN_TIJDEN: PilTijden = { taak: null, verwerktSinds: null, afgerondSinds: null };

/** Alles wat de pil nodig heeft om te beslissen. */
export interface PilInvoer {
  weergave: Weergave;
  plak: PlakUitkomst | null;
  /** Het dictaat van de sneltoets (`pil-taak`); andere taken, zoals de proef, tonen niets. */
  pilTaak: number | null;
  /** Een melding die de gebruiker heeft weggeklikt, voor dit taak-id. */
  gesloten: number | null;
  tijden: PilTijden;
  nu: number;
  /** Het laatste bericht over opschonen (`pil-opschonen`), of `null`. */
  opschonen?: PilOpschonen | null;
}

function taakVan(weergave: Weergave): number | null {
  return weergave.type === 'rust' ? null : weergave.taak;
}

/** Is de afloop van het dictaat bekend? */
function afgerond(weergave: Weergave, plak: PlakUitkomst | null): boolean {
  if (weergave.type !== 'klaar') return false;
  return weergave.uitslag.tekst === null || plak !== null;
}

/**
 * Werkt de momenten bij. Een nieuwe taak begint opnieuw; de momenten worden één keer
 * gezet, op het eerste moment dat de toestand ze laat zien.
 */
export function volgTijden(
  vorige: PilTijden,
  weergave: Weergave,
  plak: PlakUitkomst | null,
  nu: number
): PilTijden {
  const taak = taakVan(weergave);
  const basis = vorige.taak === taak ? vorige : { ...GEEN_TIJDEN, taak };
  let { verwerktSinds, afgerondSinds } = basis;
  if (weergave.type === 'verwerkt' || weergave.type === 'klaar') verwerktSinds ??= nu;
  if (afgerond(weergave, plak)) afgerondSinds ??= nu;
  if (verwerktSinds === basis.verwerktSinds && afgerondSinds === basis.afgerondSinds) {
    return basis;
  }
  return { taak, verwerktSinds, afgerondSinds };
}

function wachtend(tijden: PilTijden, nu: number): PilToestand {
  const sinds = tijden.verwerktSinds ?? nu;
  return nu - sinds > VERWERKT_NA_MS ? { type: 'verwerkt' } : { type: 'afronden' };
}

function tijdelijk(
  toestand: PilToestand,
  tijden: PilTijden,
  nu: number,
  duur: number
): PilToestand {
  const sinds = tijden.afgerondSinds ?? nu;
  return nu - sinds < duur ? toestand : { type: 'verborgen' };
}

/** Loopt het opschonen van deze taak? Dan het label van de aanbieder. */
function opschoonLabel(opschonen: PilOpschonen | null | undefined, taak: number): Label | null {
  if (opschonen?.taak !== taak || opschonen.fase.type !== 'bezig') return null;
  return opschonen.fase.label;
}

/** Bleef deze taak letterlijk na een poging tot opschonen? */
function isTerugval(opschonen: PilOpschonen | null | undefined, taak: number): boolean {
  return opschonen?.taak === taak && opschonen.fase.type === 'letterlijk';
}

/** Wat de pil nu toont. */
export function pilToestand(invoer: PilInvoer): PilToestand {
  const { weergave, plak, pilTaak, gesloten, tijden, nu, opschonen } = invoer;
  const taak = taakVan(weergave);
  if (taak === null || taak !== pilTaak || taak === gesloten) return { type: 'verborgen' };
  // Opschonen gaat voor "afronden" en "verwerkt": de gebruiker ziet dat de tekst de computer
  // verlaat, met het label van de aanbieder.
  const label = opschoonLabel(opschonen, taak);
  const wachtOpTekst = weergave.type === 'verwerkt' || (weergave.type === 'klaar' && plak === null);
  if (label !== null && wachtOpTekst) return { type: 'opschonen', label };
  switch (weergave.type) {
    case 'rust':
    case 'geannuleerd':
      return { type: 'verborgen' };
    case 'luistert': {
      const duurMs = Math.max(0, nu - weergave.sinds);
      const restMs = duurMs >= WAARSCHUWING_NA_MS ? Math.max(0, MAX_OPNAME_MS - duurMs) : null;
      return { type: 'luistert', duurMs, restMs };
    }
    case 'verwerkt':
      return wachtend(tijden, nu);
    case 'fout':
      // Blijft staan tot de gebruiker iets doet (sluiten, of een nieuw dictaat).
      return { type: 'fout', melding: weergave.melding, vervolg: weergave.vervolg };
    case 'klaar': {
      const { uitslag } = weergave;
      if (uitslag.tekst === null) {
        const soort = uitslag.uitkomst === 'tekst' ? 'geen_tekst' : uitslag.uitkomst;
        return tijdelijk(
          { type: 'klaar', soort, duurMs: uitslag.duurMs },
          tijden,
          nu,
          MELDING_ZICHTBAAR_MS
        );
      }
      // De tekst is er, het plakken nog niet: voor de gebruiker is het nog "verwerkt".
      if (plak === null) return wachtend(tijden, nu);
      switch (plakSoort(plak)) {
        case 'fout':
          return { type: 'plakfout', uitkomst: plak };
        case 'gelukt':
          return tijdelijk(
            isTerugval(opschonen, taak)
              ? { type: 'terugval', soort: 'geplakt' }
              : { type: 'klaar', soort: 'geplakt', duurMs: uitslag.duurMs },
            tijden,
            nu,
            KLAAR_ZICHTBAAR_MS
          );
        case 'klembord': {
          const soort =
            plak === 'waarschijnlijk_geplakt'
              ? 'waarschijnlijk'
              : plak === 'in_mondig'
                ? 'in_mondig'
                : 'klembord';
          return tijdelijk(
            isTerugval(opschonen, taak)
              ? { type: 'terugval', soort }
              : { type: 'klaar', soort, duurMs: uitslag.duurMs },
            tijden,
            nu,
            MELDING_ZICHTBAAR_MS
          );
        }
      }
    }
  }
}

/** Welke knop een fout in de pil krijgt: er is er hooguit één (ADR 0012). */
export type PilActie = 'kopieren' | 'microfoon' | 'mondig' | null;

export function pilActie(toestand: PilToestand): PilActie {
  if (toestand.type === 'plakfout') return 'kopieren';
  if (toestand.type !== 'fout') return null;
  switch (toestand.vervolg) {
    case 'microfooninstelling_openen':
      return 'microfoon';
    case 'opnieuw_downloaden':
    case 'ander_model_kiezen':
      return 'mondig';
    case 'opnieuw_proberen':
    case 'opnieuw_installeren':
    case 'geen':
      return null;
  }
}

/** Zo veel staafjes heeft de meter. */
export const METER_STAAFJES = 5;

/** Schuift een nieuw niveau achteraan de meter; de oudste valt eraf. */
export function schuifNiveau(niveaus: readonly number[], nieuw: number): number[] {
  const veilig = Number.isFinite(nieuw) ? Math.min(1, Math.max(0, nieuw)) : 0;
  return [...niveaus, veilig].slice(-METER_STAAFJES);
}

/** De hoogte van een staafje in px: 4 bij stilte, 18 bij volle uitsturing. */
export function staafHoogte(niveau: number): number {
  return Math.round(4 + 14 * Math.min(1, Math.max(0, niveau)));
}

/** Tijd die de pil nog over heeft, als hele seconden naar boven ("Nog 48 s"). */
export function restSeconden(restMs: number): number {
  return Math.ceil(restMs / 1000);
}

/** Hoe lang tot de pil iets anders moet tonen zonder nieuw event, of `null` als nooit. */
export function volgendeTik(invoer: PilInvoer): number | null {
  const toestand = pilToestand(invoer);
  if (toestand.type === 'luistert') return 250;
  const { weergave, plak, tijden, nu } = invoer;
  const wachtOpVerwerkt =
    weergave.type === 'verwerkt' || (weergave.type === 'klaar' && !afgerond(weergave, plak));
  if (wachtOpVerwerkt && toestand.type === 'afronden') {
    return Math.max(0, (tijden.verwerktSinds ?? nu) + VERWERKT_NA_MS + 1 - nu);
  }
  if (toestand.type === 'klaar') {
    const duur = toestand.soort === 'geplakt' ? KLAAR_ZICHTBAAR_MS : MELDING_ZICHTBAAR_MS;
    return Math.max(0, (tijden.afgerondSinds ?? nu) + duur - nu);
  }
  if (toestand.type === 'terugval') {
    const duur = toestand.soort === 'geplakt' ? KLAAR_ZICHTBAAR_MS : MELDING_ZICHTBAAR_MS;
    return Math.max(0, (tijden.afgerondSinds ?? nu) + duur - nu);
  }
  return null;
}
