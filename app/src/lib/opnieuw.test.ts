import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import { eersteKeuze, keuzeRegel, opnieuwGeslaagd, rolTekst, voortgangTekst } from './opnieuw';
import type { OpnieuwKeuze, StandToestand } from './vergadering';

const nauwkeurig: OpnieuwKeuze = {
  modelId: 'canary',
  rol: 'nauwkeurig',
  aanwezig: true,
  grootteBytes: 1_144_290_016,
  duurMs: 9 * 60_000
};
const extra: OpnieuwKeuze = {
  modelId: 'whisper',
  rol: 'extra_nauwkeurig',
  aanwezig: false,
  grootteBytes: 886_381_728,
  duurMs: null
};

describe('opnieuw uitschrijven: de hulpjes', () => {
  it('noemt de rol als bijvoeglijk naamwoord', () => {
    expect(rolTekst('snel', nl)).toBe('snelle');
    expect(rolTekst('extra_nauwkeurig', nl)).toBe('extra nauwkeurige');
    expect(rolTekst('nauwkeurig', en)).toBe('accurate');
    expect(rolTekst(null, nl)).toBeNull();
  });

  it('geeft per keuze de tijd, de eerlijke zin zonder meting, of de downloadgrootte', () => {
    expect(keuzeRegel(nauwkeurig, 52 * 60_000, 'snel', nl, 'nl')).toBe(
      'Minder fouten. Duurt ongeveer 9 minuten voor deze 52 minuten.'
    );
    expect(keuzeRegel({ ...extra, aanwezig: true }, 52 * 60_000, 'nauwkeurig', nl, 'nl')).toBe(
      'De minste fouten. Duurt langer dan het nauwkeurige model.'
    );
    expect(keuzeRegel(extra, 52 * 60_000, 'snel', nl, 'nl')).toBe(
      'Nog niet op deze computer. Eerst downloaden: 886 MB.'
    );
    expect(keuzeRegel(nauwkeurig, 60_000, 'snel', en, 'en')).toBe(
      'Fewer mistakes. Takes about 9 minutes for these 1 minute.'
    );
  });

  it('kiest vooraf het eerste model dat er al is', () => {
    expect(eersteKeuze([extra, nauwkeurig])).toBe('canary');
    expect(eersteKeuze([extra])).toBe('whisper');
    expect(eersteKeuze([])).toBeNull();
  });

  it('maakt de regel van de voortgang', () => {
    expect(voortgangTekst('nauwkeurig', 34, 360_000, nl)).toBe(
      'Opnieuw uitschrijven met het nauwkeurige model… 34% · nog ongeveer 6 minuten.'
    );
    expect(voortgangTekst('nauwkeurig', 100, 0, nl)).toBe(
      'Opnieuw uitschrijven met het nauwkeurige model… 100%.'
    );
    expect(voortgangTekst(null, null, null, nl)).toBe('Opnieuw uitschrijven…');
  });
});

describe('opnieuw uitschrijven geslaagd', () => {
  const verslag = (opnieuw: unknown, naOpnieuw: unknown) =>
    ({ type: 'verslag', opnieuw, naOpnieuw }) as unknown as StandToestand;
  const loopt = verslag({ voortgang: 10 }, null);
  const klaar = verslag(null, { rol: null, veranderd: 1, verdwenen: 0 });

  it('telt alleen een opnieuw uitschrijven dat liep en met een melding eindigde', () => {
    expect(opnieuwGeslaagd(loopt, klaar)).toBe(true);
  });

  it('telt stoppen niet: zonder melding is het verslag niet veranderd', () => {
    expect(opnieuwGeslaagd(loopt, verslag(null, null))).toBe(false);
  });

  it('telt een stand zonder opnieuw uitschrijven ervoor niet', () => {
    expect(opnieuwGeslaagd(klaar, klaar)).toBe(false);
    expect(opnieuwGeslaagd(undefined, klaar)).toBe(false);
    expect(opnieuwGeslaagd({ type: 'klaar' } as unknown as StandToestand, klaar)).toBe(false);
  });
});
