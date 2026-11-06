import type { KnopFase, RijToestand } from './downloadrij';
import type { ModellenOverzicht } from './modellen';

// De logica rond de eerste-start-wizard (fase 5): wanneer hij verschijnt, en hoe lang. Het
// scherm zelf staat in `EersteStart.svelte`.

/** Is er al een spraakmodel dat helemaal op de computer staat? */
export function heeftSpraakmodel(overzicht: ModellenOverzicht): boolean {
  return overzicht.modellen.some((m) => m.soort === 'spraak' && m.status === 'compleet');
}

/**
 * Moet de wizard nu zichtbaar zijn? Pas als het overzicht binnen is (anders flitst hij bij
 * iedereen kort), en alleen als er nog geen model staat. Eenmaal zichtbaar blijft hij staan tot
 * de gebruiker klaar is: de download maakt het model compleet, maar de laatste stappen
 * (videokaart, sneltoets) volgen nog. Wie klaar was of nu niet koos, ziet hem niet opnieuw
 * tijdens deze sessie.
 */
export function wizardZichtbaar(
  overzicht: ModellenOverzicht | null,
  eerderZichtbaar: boolean,
  afgesloten: boolean
): boolean {
  if (afgesloten) return false;
  if (eerderZichtbaar) return true;
  return overzicht !== null && !heeftSpraakmodel(overzicht);
}

/** De stappen van de wizard, in volgorde. */
export type WizardStap = 'waarom' | 'advies' | 'downloaden' | 'klaar';

export const WIZARD_STAPPEN: readonly WizardStap[] = ['waarom', 'advies', 'downloaden', 'klaar'];

/** Het volgnummer (vanaf 1) van een stap, voor "Stap 2 van 4". */
export function stapNummer(stap: WizardStap): number {
  return WIZARD_STAPPEN.indexOf(stap) + 1;
}

/**
 * Welke taken het eerste model krijgt: dicteren, en ook bestanden als het advies voor
 * bestanden geen tweede model is (dan is er maar één download en doet dat model beide).
 */
export function takenEersteModel(bestandenApart: boolean): ('dicteren' | 'bestanden')[] {
  return bestandenApart ? ['dicteren'] : ['dicteren', 'bestanden'];
}

/**
 * Welke modellen de wizard downloadt, in volgorde. Ze gaan meteen allebei in de downloadrij en
 * komen daar na elkaar binnen. Het tweede mag je overslaan.
 */
export function teDownloaden(
  advies: { dicteren: string; bestanden: string; bestandenApart: boolean },
  metTweede: boolean
): string[] {
  return advies.bestandenApart && metTweede
    ? [advies.dicteren, advies.bestanden]
    : [advies.dicteren];
}

/** Hoe een model van de wizard ervoor staat: op schijf, in de downloadrij, en of er een fout is. */
export interface WizardModel {
  id: string;
  /** Staat het model helemaal op de computer (volgens het overzicht van schijf)? */
  compleet: boolean;
  /** Wat de Downloaden-knop van dit model zou zeggen (zie `knopFase`). */
  fase: KnopFase;
  /** De toestand van zijn item in de rij, als het er een heeft. */
  toestand: RijToestand['type'] | null;
  /** De melding van de laatste mislukte poging, of `null`. */
  fout: string | null;
}

/**
 * Waar de downloads van de wizard staan. Alleen een model dat op schijf compleet is telt als
 * klaar; een item dat "klaar" meldt terwijl het overzicht dat nog niet ziet, wordt eerst
 * gecontroleerd. Zolang een van de modellen wacht, binnenkomt of net gestart is, loopt het
 * nog (`model` is dan het model om te tonen: wat binnenkomt, anders wat wacht). Pas als niets
 * meer loopt, telt een fout; en zonder fout is het gestopt (in de zijbalk, onderbroken, of
 * nooit gestart).
 */
export type WizardDownload =
  | { soort: 'bezig'; model: string }
  | { soort: 'controleren'; model: string }
  | { soort: 'fout'; model: string; melding: string }
  | { soort: 'gestopt' }
  | { soort: 'klaar' };

export function downloadStand(modellen: readonly WizardModel[]): WizardDownload {
  const open = modellen.filter((m) => !m.compleet);
  if (open.length === 0) return { soort: 'klaar' };
  const binnen = open.find((m) => m.fase === 'bezig' || m.fase === 'opnieuw');
  const wacht = open.find((m) => m.fase === 'wacht' || m.fase === 'wordt_gestart');
  const loopt = binnen ?? wacht;
  if (loopt) return { soort: 'bezig', model: loopt.id };
  const controle = open.find((m) => m.toestand === 'klaar');
  if (controle) return { soort: 'controleren', model: controle.id };
  const mislukt = open.find((m) => m.fout !== null);
  if (mislukt?.fout) return { soort: 'fout', model: mislukt.id, melding: mislukt.fout };
  return { soort: 'gestopt' };
}

/** Wat de wizard van één model van het plan weet, om de voortgang van alles samen te tellen. */
export interface WizardMaat {
  /** Wat er in totaal te downloaden is. */
  grootteBytes: number;
  /** Wat er nu al op schijf staat, ook van een afgebroken download. */
  aanwezigBytes: number;
  /**
   * Wat er op schijf stond toen het downloaden begon (bij een klik op Downloaden). Daar hoort
   * het totaal van te zijn: bij een hervatte download is dat minder dan de hele grootte.
   */
  startAanwezigBytes: number;
  /** Staat het model helemaal op de computer? */
  compleet: boolean;
  /** Wat de rij van dit model tot nu toe binnen heeft, of `null` als het geen voortgang meldt. */
  binnenBytes: number | null;
}

/** Wat er nog te halen was bij de start van het downloaden: het getal op de knop. */
export function teHalenBytes(
  modellen: readonly Pick<WizardMaat, 'grootteBytes' | 'startAanwezigBytes'>[]
): number {
  return modellen.reduce((som, m) => som + Math.max(0, m.grootteBytes - m.startAanwezigBytes), 0);
}

/**
 * De voortgang van alle downloads van de wizard samen, in bytes. De wizard toont dit en niet
 * de voortgang van het ene model dat nu binnenkomt: de knop zei "1,8 GB", dus de voortgang
 * hoort "van 1,8 GB" te zijn en niet "van 742 MB". Het totaal is wat er bij de start nog te
 * halen was (`teHalenBytes`), dus altijd gelijk aan wat de knop noemde, ook bij een hervatte
 * download. `binnen` is wat er sindsdien bij is gekomen: een model dat klaar is telt voor zijn
 * hele rest, een model dat binnenkomt voor wat de rij meldt (nooit minder dan wat al op schijf
 * stond, nooit meer dan het model zelf groot is).
 */
export function totaalVoortgang(modellen: readonly WizardMaat[]): {
  binnen: number;
  totaal: number;
} {
  let binnen = 0;
  for (const m of modellen) {
    const nu = m.compleet
      ? m.grootteBytes
      : Math.min(m.grootteBytes, Math.max(m.aanwezigBytes, m.binnenBytes ?? 0));
    binnen += Math.max(0, nu - m.startAanwezigBytes);
  }
  return { binnen, totaal: teHalenBytes(modellen) };
}
