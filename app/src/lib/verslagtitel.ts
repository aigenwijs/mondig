import type { Berichten } from './i18n/typen';
import { duurMinuten, bewaarTot, dagVan, type Vergadersoort } from './vergadering';

// De teksten rond een bewaard verslag (N-C3, ADR 0022): de titel, de gegevensregel in de lijst
// "Eerder opgenomen" en de zin "Mondig bewaart dit verslag tot ...". Pure hulpjes, zodat de lijst
// en het verslag dezelfde tekst tonen en ze zonder scherm te testen zijn.
//
// De titel volgt Klant en Onderwerp (UI-E, keuze A): "Gemeente Vlaardingen · Intake". Zijn beide
// leeg, dan alleen het soortwoord ("Vergadering", "Notitie"), met de datum in de metaregel.

/** De teksten die deze hulpjes nodig hebben. */
export type VerslagTeksten = Pick<
  Berichten,
  | 'titelVergadering'
  | 'titelNotitie'
  | 'vergaderWanneer'
  | 'vergaderDagVandaag'
  | 'vergaderDagGisteren'
  | 'vergaderDagMorgen'
  | 'vergaderMinutenKort'
  | 'verslagSprekers'
  | 'verslagAlleenIk'
  | 'vergaderInDeRuimte'
  | 'verslagVerwijderdOp'
  | 'verslagVerwijderdVandaagOm'
  | 'verslagVerwijderdMorgenOm'
  | 'verslagVerwijderdVandaag'
>;

/** Wat een verslag nodig heeft voor zijn titel en zijn gegevensregel. */
export interface VerslagGegevens {
  /** De klant; leeg zonder klant. */
  klant: string;
  /**
   * Het onderwerp: wat de gebruiker invulde, of `null` als hij het nooit aanraakte.
   */
  onderwerp: string | null;
  soort: Vergadersoort;
  beginMs: number;
  duurMs: number;
  /** Hoeveel sprekers er zijn, als ze herkend zijn. */
  sprekers: number | null;
}

/** "2 november": de dag en de maand. */
export function datumTekst(ms: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'long' }).format(new Date(ms));
}

/** "14:02": het uur en de minuut. */
export function tijdTekst(ms: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false
  }).format(new Date(ms));
}

/** Het soortwoord in de taal van de app: "Vergadering" of "Notitie". */
export function soortwoord(
  soort: Vergadersoort,
  t: Pick<VerslagTeksten, 'titelVergadering' | 'titelNotitie'>
): string {
  return soort === 'alleen' ? t.titelNotitie : t.titelVergadering;
}

/**
 * De titel van een verslag (keuze A): de niet-lege waarden van Klant en Onderwerp met " · "
 * ertussen. Een onderwerp dat nooit is aangeraakt is het soortwoord ("Gemeente Vlaardingen ·
 * Vergadering"). Zijn beide leeg, dan alleen het soortwoord. Het open verslag geeft de
 * waarden mee zoals de kop ze toont (met Vervangen aan: de plaatsvervangers).
 */
export function verslagTitel(
  verslag: Pick<VerslagGegevens, 'klant' | 'onderwerp' | 'soort'>,
  t: Pick<VerslagTeksten, 'titelVergadering' | 'titelNotitie'>
): string {
  const woord = soortwoord(verslag.soort, t);
  // `null`: nooit aangeraakt, dan is het onderwerp het soortwoord.
  const onderwerp = verslag.onderwerp?.trim() ?? woord;
  const delen = [verslag.klant.trim(), onderwerp].filter((d) => d !== '');
  return delen.length > 0 ? delen.join(' · ') : woord;
}

/** "vandaag, 14:02", "gisteren, 14:02" of "12 oktober, 14:02". */
export function wanneerTekst(
  ms: number,
  nu: number,
  t: Pick<VerslagTeksten, 'vergaderWanneer' | 'vergaderDagVandaag' | 'vergaderDagGisteren'>,
  locale: string
): string {
  const dag = dagVan(ms, nu);
  const woord =
    dag === 'vandaag'
      ? t.vergaderDagVandaag
      : dag === 'gisteren'
        ? t.vergaderDagGisteren
        : datumTekst(ms, locale);
  return t.vergaderWanneer(woord, tijdTekst(ms, locale));
}

/** Zet de eerste letter van een regel in hoofdletters. */
export function hoofdletter(tekst: string, locale: string): string {
  return tekst.charAt(0).toLocaleUpperCase(locale) + tekst.slice(1);
}

/**
 * De gegevens onder de titel in de lijst, zoals het ontwerp ze toont: "Vandaag, 14:00", "52 min" en
 * de sprekers ("3 sprekers"), "alleen ik" of "In de ruimte". Online zonder herkende sprekers
 * heeft geen derde deel.
 */
export function gegevensDelen(
  verslag: Pick<VerslagGegevens, 'soort' | 'beginMs' | 'duurMs' | 'sprekers'>,
  nu: number,
  t: VerslagTeksten,
  locale: string
): string[] {
  const delen = [
    hoofdletter(wanneerTekst(verslag.beginMs, nu, t, locale), locale),
    t.vergaderMinutenKort(duurMinuten(verslag.duurMs))
  ];
  if (verslag.sprekers !== null) delen.push(t.verslagSprekers(verslag.sprekers));
  else if (verslag.soort === 'alleen') delen.push(t.verslagAlleenIk);
  else if (verslag.soort === 'ruimte') delen.push(t.vergaderInDeRuimte);
  return delen;
}

/**
 * Wanneer Mondig het verslag verwijdert, in gewone woorden: "Wordt vandaag om 14:00 verwijderd",
 * "Wordt morgen om 14:00 verwijderd" of "Wordt op 12 november verwijderd". `null` als Mondig de
 * bewaartermijn niet kent en dus niets verwijdert. Is het moment al voorbij (de klok liep terug
 * of Mondig was uit), dan staat er geen tijd: "Wordt vandaag verwijderd".
 */
export function bewaarTotTekst(
  wistMs: number | null,
  nu: number,
  t: Pick<
    VerslagTeksten,
    | 'verslagVerwijderdOp'
    | 'verslagVerwijderdVandaagOm'
    | 'verslagVerwijderdMorgenOm'
    | 'verslagVerwijderdVandaag'
  >,
  locale: string
): string | null {
  const tot = bewaarTot(wistMs, nu);
  if (tot === null || wistMs === null) return null;
  if (tot === 'vandaag') {
    return wistMs > nu
      ? t.verslagVerwijderdVandaagOm(tijdTekst(wistMs, locale))
      : t.verslagVerwijderdVandaag;
  }
  if (tot === 'morgen') return t.verslagVerwijderdMorgenOm(tijdTekst(wistMs, locale));
  return t.verslagVerwijderdOp(datumTekst(wistMs, locale));
}
