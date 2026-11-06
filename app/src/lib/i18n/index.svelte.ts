import { en } from './en';
import { nl } from './nl';
import type { Berichten, Taal } from './typen';

export type { Berichten, Taal } from './typen';

/** Alle talen met hun berichten. */
export const talen: Record<Taal, Berichten> = { nl, en };

// Actieve taal als reactieve state; geen window- of document-afhankelijkheid.
const staat = $state<{ taal: Taal }>({ taal: 'nl' });

/** Berichten van de actieve taal. Roep aan in de template voor een reactieve tekst. */
export function t(): Berichten {
  return talen[staat.taal];
}

/** De actieve taal. */
export function huidigeTaal(): Taal {
  return staat.taal;
}

/** Wisselt de actieve taal. */
export function zetTaal(taal: Taal): void {
  staat.taal = taal;
}
