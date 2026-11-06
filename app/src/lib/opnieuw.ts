import { leesbareGrootte } from './grootte';
import type { Berichten } from './i18n/typen';
import { resterendeTijdTekst } from './resterendetijd';
import { duurMinuten, type Modelrol, type OpnieuwKeuze } from './vergadering';

// Opnieuw uitschrijven (N-C4, ADR 0023): de pure hulpjes voor de dialoog, de voortgang en de
// melding. Een model heet hier bij zijn rol, nooit bij zijn naam (N-B).

/** De rol als bijvoeglijk naamwoord ("nauwkeurige"), of `null` als die niet bekend is. */
export function rolTekst(rol: Modelrol | null, t: Berichten): string | null {
  return rol === null ? null : t.opnieuwRol(rol);
}

/**
 * De kleine regel onder een keuze in de dialoog: wat het oplevert en hoe lang het duurt voor deze
 * opname, of de downloadgrootte als het model er nog niet is. Zonder meting een eerlijke zin in
 * plaats van een verzonnen getal.
 */
export function keuzeRegel(
  keuze: OpnieuwKeuze,
  opnameMs: number,
  huidigeRol: Modelrol | null,
  t: Berichten,
  taal: 'nl' | 'en'
): string {
  if (!keuze.aanwezig) return t.opnieuwOntbreekt(leesbareGrootte(keuze.grootteBytes, taal));
  const duur =
    keuze.duurMs === null
      ? t.opnieuwDuurOnbekend(rolTekst(huidigeRol, t))
      : t.opnieuwDuur(duurMinuten(keuze.duurMs), duurMinuten(opnameMs));
  return `${t.opnieuwVoordeel(keuze.rol)} ${duur}`;
}

/** De keuze die de dialoog vooraf aanvinkt: het eerste model dat er al is, anders het eerste. */
export function eersteKeuze(keuzes: readonly OpnieuwKeuze[]): string | null {
  return (keuzes.find((k) => k.aanwezig) ?? keuzes[0])?.modelId ?? null;
}

/**
 * De regel in de voortgangsbalk: "Opnieuw uitschrijven met het nauwkeurige model… 34% · nog
 * ongeveer 6 minuten." Zonder percentage of tijd valt dat deel weg.
 */
export function voortgangTekst(
  rol: Modelrol | null,
  voortgang: number | null,
  restMs: number | null,
  t: Berichten
): string {
  const delen: string[] = [];
  if (voortgang !== null) delen.push(t.procent(voortgang));
  if (restMs !== null && restMs > 0) {
    const rest = resterendeTijdTekst(Math.round(restMs / 1000), t);
    delen.push(rest.charAt(0).toLowerCase() + rest.slice(1));
  }
  const kop = t.opnieuwBezig(rolTekst(rol, t));
  return delen.length === 0 ? kop : `${kop} ${delen.join(' · ')}.`;
}
