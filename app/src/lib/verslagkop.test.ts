import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import type { ModelRegel } from './modellen';
import type { StandToestand } from './vergadering';
import {
  KLAAR_MOMENT_MS,
  aantalSprekers,
  bepaalKlaarMoment,
  volgUitschrijfStart,
  klaarZin,
  modelRol,
  startKlaarTimer
} from './verslagkop';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// De fixture komt uit het echte modellen.json, zoals Rust het teruggeeft: de naam zonder de
// uitleg tussen haakjes, de balken en de labels. Zo test dit tegen wat de app echt krijgt.
interface BronModel {
  id: string;
  naam: string;
  soort: string;
  snelheid: number | null;
  nauwkeurigheid: number | null;
}
const bron = JSON.parse(
  readFileSync(
    fileURLToPath(new URL('../../../crates/mondig-core/src/modellen.json', import.meta.url)),
    'utf8'
  )
) as { modellen: BronModel[] };

const modellen = bron.modellen
  .filter((m) => m.soort === 'spraak')
  .map(
    (m) =>
      ({
        id: m.id,
        naam: m.naam.replace(/\s*\(.*$/, ''),
        soort: m.soort,
        snelheid: m.snelheid,
        nauwkeurigheid: m.nauwkeurigheid
      }) as unknown as ModelRegel
  );

const PARAKEET = 'parakeet-ultra-q8';
const CANARY = 'canary-1b-v2-q8';
const WHISPER = 'whisper-large-v3-turbo-q8';
const overzicht = (dicteren: string, bestanden: string) => ({ modellen, dicteren, bestanden });

describe('modelRol', () => {
  it('heeft de echte modeldata: Canary heeft gelijke balken', () => {
    const canary = modellen.find((m) => m.id === CANARY);
    expect(canary?.naam).toBe('Canary 1b-v2');
    expect(canary?.snelheid).toBe(canary?.nauwkeurigheid);
  });

  it('noemt het model voor bestanden nauwkeurig en dat voor dicteren snel', () => {
    const o = overzicht(PARAKEET, CANARY);
    expect(modelRol('Canary 1b-v2', o)).toBe('nauwkeurig');
    expect(modelRol('Parakeet Ultra', o)).toBe('snel');
  });

  it('herkent een korte naam uit het verslag', () => {
    const o = overzicht(PARAKEET, CANARY);
    expect(modelRol('Canary', o)).toBe('nauwkeurig');
    expect(modelRol('Parakeet', o)).toBe('snel');
  });

  it('valt voor een model dat niet meer in gebruik is terug op de balken', () => {
    const o = overzicht(PARAKEET, CANARY);
    expect(modelRol('Whisper large-v3 turbo', o)).toBe('nauwkeurig');
    expect(modelRol('Parakeet Ultra', overzicht(CANARY, WHISPER))).toBe('snel');
  });

  it('geeft null als er maar één model voor alles is of de rol onduidelijk is', () => {
    expect(modelRol('Parakeet Ultra', overzicht(PARAKEET, PARAKEET))).toBeNull();
    // Canary heeft gelijke balken en is hier geen van beide modellen in gebruik.
    expect(modelRol('Canary 1b-v2', overzicht(PARAKEET, WHISPER))).toBeNull();
    expect(modelRol('Onbekend', overzicht(PARAKEET, CANARY))).toBeNull();
    expect(modelRol('', overzicht(PARAKEET, CANARY))).toBeNull();
    expect(modelRol('Canary', null)).toBeNull();
    expect(modelRol('Canary', { modellen: [], dicteren: PARAKEET, bestanden: CANARY })).toBeNull();
  });
});

describe('meervoud en rollen in de teksten', () => {
  it('zegt 1 spreker in het enkelvoud', () => {
    expect(nl.verslagSprekers(1)).toBe('1 spreker');
    expect(nl.verslagSprekers(3)).toBe('3 sprekers');
    expect(en.verslagSprekers(1)).toBe('1 speaker');
    expect(en.verslagSprekers(2)).toBe('2 speakers');
  });

  it('noemt in Instellingen rollen en geen modelnamen', () => {
    expect(nl.instellingModellenHulp(true)).toBe(
      'Dicteren met het snelle model, opnames en bestanden met het nauwkeurige.'
    );
    expect(nl.instellingModellenHulp(false)).toBe(
      'Dicteren, opnames en bestanden met hetzelfde model.'
    );
  });
});

describe('aantalSprekers', () => {
  it('telt alleen herkende sprekers, met Jij erbij', () => {
    expect(
      aantalSprekers([
        { kant: 'jij', sprekernummer: null },
        { kant: 'anderen', sprekernummer: 1 },
        { kant: 'anderen', sprekernummer: 2 },
        { kant: 'anderen', sprekernummer: 1 }
      ])
    ).toBe(3);
  });

  it('geeft null zolang de sprekers niet herkend zijn', () => {
    expect(
      aantalSprekers([
        { kant: 'jij', sprekernummer: null },
        { kant: 'anderen', sprekernummer: null }
      ])
    ).toBeNull();
  });
});

const verslag = { type: 'verslag', duurMs: 52 * 60_000 } as unknown as StandToestand;
const uitschrijven = { type: 'uitschrijven' } as unknown as StandToestand;

describe('klaar-moment', () => {
  it('hoort alleen bij een uitschrijven dat het scherm zag eindigen', () => {
    expect(bepaalKlaarMoment('uitschrijven', verslag, 1000, 241_000)).toEqual({
      gesprekMs: 52 * 60_000,
      uitschrijfMs: 240_000
    });
    expect(bepaalKlaarMoment(undefined, verslag, null, 5)).toBeNull();
    expect(bepaalKlaarMoment('verslag', verslag, null, 5)).toBeNull();
    expect(bepaalKlaarMoment('uitschrijven', uitschrijven, 0, 5)).toBeNull();
  });

  it('laat de uitschrijftijd weg als het scherm het begin niet zag', () => {
    expect(bepaalKlaarMoment('uitschrijven', verslag, null, 5)?.uitschrijfMs).toBeNull();
  });

  it('telt het uitschrijven alleen als het scherm het zag beginnen', () => {
    // Overgang gezien: de start is het moment van de overgang en blijft staan.
    const start = volgUitschrijfStart('voorbereiden', 'uitschrijven', null, 1000);
    expect(start).toBe(1000);
    expect(volgUitschrijfStart('uitschrijven', 'uitschrijven', start, 9000)).toBe(1000);
    // Het scherm opent midden in het uitschrijven (of na wisselen van scherm): geen start.
    const midden = volgUitschrijfStart(undefined, 'uitschrijven', null, 5000);
    expect(midden).toBeNull();
    expect(volgUitschrijfStart('uitschrijven', 'uitschrijven', midden, 9000)).toBeNull();
    // Daarna geen tijd in het klaar-moment, geen verkeerde.
    expect(bepaalKlaarMoment('uitschrijven', verslag, midden, 250_000)?.uitschrijfMs).toBeNull();
    // Buiten het uitschrijven is er geen start.
    expect(volgUitschrijfStart('uitschrijven', 'verslag', start, 9000)).toBeNull();
  });

  it('zegt in gewone taal wat er gebeurde', () => {
    expect(klaarZin({ gesprekMs: 52 * 60_000, uitschrijfMs: 240_000 }, nl)).toBe(
      '52 minuten gesprek, in 4 minuten uitgeschreven. Alles bleef op deze computer.'
    );
    expect(klaarZin({ gesprekMs: 60_000, uitschrijfMs: 20_000 }, nl)).toBe(
      '1 minuut gesprek, in minder dan een minuut uitgeschreven. Alles bleef op deze computer.'
    );
    expect(klaarZin({ gesprekMs: 52 * 60_000, uitschrijfMs: null }, nl)).toBe(
      '52 minuten gesprek. Alles bleef op deze computer.'
    );
  });
});

describe('het klaar-moment verdwijnt vanzelf', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('na zes seconden', () => {
    vi.useFakeTimers();
    const weg = vi.fn();
    startKlaarTimer(weg);
    vi.advanceTimersByTime(KLAAR_MOMENT_MS - 1);
    expect(weg).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(weg).toHaveBeenCalledTimes(1);
    expect(KLAAR_MOMENT_MS).toBe(6000);
  });

  it('niet meer als het scherm eerder wegging', () => {
    vi.useFakeTimers();
    const weg = vi.fn();
    const stop = startKlaarTimer(weg);
    stop();
    vi.advanceTimersByTime(KLAAR_MOMENT_MS * 2);
    expect(weg).not.toHaveBeenCalled();
  });
});
