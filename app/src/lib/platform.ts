import { haalSneltoetsInfo, type Platform } from './sneltoets';

// Het platform verandert niet terwijl de app draait: één vraag aan Rust voor iedereen die het wil
// weten (de knoppen "Tonen in Verkenner", "Wat is er nieuw", Over Mondig). Mislukt de vraag, dan
// probeert de volgende aanroep opnieuw. Buiten de app (gewone browser) is er geen antwoord: `null`.
let vraag: Promise<Platform | null> | null = null;

export function haalPlatform(): Promise<Platform | null> {
  vraag ??= haalSneltoetsInfo()
    .then((info) => info?.platform ?? null)
    .catch(() => {
      vraag = null;
      return null;
    });
  return vraag;
}

/** Alleen voor de tests: vergeet het onthouden antwoord. */
export function vergeetPlatform(): void {
  vraag = null;
}
