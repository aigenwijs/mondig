// Notatie van duur en datum in een metaregel (ontwerpsysteem §4). Los van het onderdeel, zodat
// het zonder scherm te testen is.

import type { Taal } from './i18n/typen';

/**
 * Een duur voor in een metaregel: "45 s", "38 min", "1 u 12 min". Nooit "23:14": dat leest als
 * een kloktijd. Onder de minuut in seconden, daarboven afgerond op hele minuten. In het Engels
 * is het uur "h".
 */
export function duurKort(ms: number, taal: Taal = 'nl'): string {
  const seconden = Math.max(0, Math.round(ms / 1000));
  if (seconden < 60) return `${String(seconden)} s`;
  const minuten = Math.round(seconden / 60);
  const uurAfkorting = taal === 'nl' ? 'u' : 'h';
  if (minuten < 60) return `${String(minuten)} min`;
  const uren = Math.floor(minuten / 60);
  const rest = minuten % 60;
  return rest === 0
    ? `${String(uren)} ${uurAfkorting}`
    : `${String(uren)} ${uurAfkorting} ${String(rest)} min`;
}

/**
 * Een datum voor in een metaregel: "30 okt", en met het jaar als het niet dit jaar is:
 * "30 okt 2025". Met `metTijd` (bij een opname, waar het moment telt) staat de tijd erachter:
 * "2 nov, 14:00". `nu` is het moment van nu (standaard de klok), voor de tests.
 */
export function datumKort(
  ms: number,
  taal: Taal = 'nl',
  nu: number = Date.now(),
  metTijd = false
): string {
  const datum = new Date(ms);
  const locale = taal === 'nl' ? 'nl-NL' : 'en-GB';
  const ditJaar = datum.getFullYear() === new Date(nu).getFullYear();
  const dag = new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'short',
    ...(ditJaar ? {} : { year: 'numeric' })
  }).format(datum);
  if (!metTijd) return dag;
  const tijd = new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false
  }).format(datum);
  return `${dag}, ${tijd}`;
}
