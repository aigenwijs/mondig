import type { Berichten } from './i18n/typen';
import { duurMinuten, bewaarTot, dagVan, type Vergadersoort } from './vergadering';

// De teksten rond een bewaard verslag (N-C3, ADR 0022): de titel, de gegevensregel in de lijst
// "Eerder opgenomen" en de zin "Mondig bewaart dit verslag tot ...". Pure hulpjes, zodat de lijst
// en het verslag dezelfde tekst tonen en ze zonder scherm te testen zijn.

/** De teksten die deze hulpjes nodig hebben. */
export type VerslagTeksten = Pick<
  Berichten,
  | 'notitieVan'
  | 'vergaderingVan'
  | 'vergaderWanneer'
  | 'vergaderDagVandaag'
  | 'vergaderDagGisteren'
  | 'vergaderDagMorgen'
  | 'vergaderMinutenKort'
  | 'verslagSprekers'
  | 'verslagAlleenIk'
  | 'vergaderInDeRuimte'
  | 'verslagBewaardTot'
>;

/** Wat een verslag nodig heeft voor zijn titel en zijn gegevensregel. */
export interface VerslagGegevens {
  naam: string | null;
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

/** De titel die Mondig zelf maakt: "Vergadering van 2 november, 14:00", of "Notitie van ..." bij Alleen ik. */
export function standaardTitel(
  soort: Vergadersoort,
  beginMs: number,
  t: Pick<VerslagTeksten, 'notitieVan' | 'vergaderingVan'>,
  locale: string
): string {
  return (soort === 'alleen' ? t.notitieVan : t.vergaderingVan)(
    datumTekst(beginMs, locale),
    tijdTekst(beginMs, locale)
  );
}

/** De titel van een verslag: de naam die de gebruiker gaf, anders de titel die Mondig zelf maakt. */
export function verslagTitel(
  verslag: Pick<VerslagGegevens, 'naam' | 'soort' | 'beginMs'>,
  t: Pick<VerslagTeksten, 'notitieVan' | 'vergaderingVan'>,
  locale: string
): string {
  const naam = verslag.naam?.trim() ?? '';
  return naam === '' ? standaardTitel(verslag.soort, verslag.beginMs, t, locale) : naam;
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
 * de sprekers ("3 sprekers"), "alleen ik" of "Fysieke vergadering". Online zonder herkende sprekers
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
 * "Mondig bewaart dit verslag tot dinsdag": de dag waarop Mondig het wist, als weekdag binnen een
 * week en anders als datum. `null` als Mondig de bewaartermijn niet kent en dus niets wist.
 */
export function bewaarTotTekst(
  wistMs: number | null,
  nu: number,
  t: Pick<VerslagTeksten, 'verslagBewaardTot' | 'vergaderDagVandaag' | 'vergaderDagMorgen'>,
  locale: string
): string | null {
  const tot = bewaarTot(wistMs, nu);
  if (tot === null) return null;
  let dag: string;
  if (tot.soort === 'vandaag') dag = t.vergaderDagVandaag;
  else if (tot.soort === 'morgen') dag = t.vergaderDagMorgen;
  else if (tot.soort === 'weekdag') {
    dag = new Intl.DateTimeFormat(locale, { weekday: 'long' }).format(new Date(tot.ms));
  } else dag = datumTekst(tot.ms, locale);
  return t.verslagBewaardTot(dag);
}
