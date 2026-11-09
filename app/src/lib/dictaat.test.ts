import { describe, expect, it } from 'vitest';
import {
  RUST,
  WAARSCHUWING_NA_MS,
  bijnaVol,
  formatDuur,
  isActief,
  klokTijd,
  opGebeurtenis,
  opUitslag,
  plakHoortBij,
  plakSoort,
  stripToestand,
  type Weergave
} from './dictaat';
import type { Gebeurtenis, ProefResultaat, Toestand } from './taak';

function gebeurtenis(
  taak: number,
  toestand: Toestand,
  soort: Gebeurtenis['soort'] = 'dictaat'
): Gebeurtenis {
  return { taak, soort, toestand };
}

const uitslag = (taak: number): ProefResultaat => ({
  taak,
  uitkomst: 'tekst',
  tekst: 'Hallo wereld.',
  duurMs: 1200,
  soevereiniteit: 'lokaal'
});

describe('opGebeurtenis', () => {
  it('begint met luistert en onthoudt wanneer', () => {
    const w = opGebeurtenis(RUST, gebeurtenis(3, { type: 'luistert' }), 5000);
    expect(w).toEqual({ type: 'luistert', taak: 3, sinds: 5000 });
  });

  it('volgt luistert, verwerkt en wacht dan op de uitslag', () => {
    let w: Weergave = opGebeurtenis(RUST, gebeurtenis(3, { type: 'luistert' }), 0);
    w = opGebeurtenis(w, gebeurtenis(3, { type: 'verwerkt', voortgang: null }), 1);
    expect(w).toEqual({ type: 'verwerkt', taak: 3 });
    // Het taak-event "klaar" verandert niets: de tekst komt met de uitslag.
    expect(opGebeurtenis(w, gebeurtenis(3, { type: 'klaar' }), 2)).toBe(w);
    expect(opUitslag(w, uitslag(3))).toEqual({ type: 'klaar', taak: 3, uitslag: uitslag(3) });
  });

  it('negeert een uitslag van een ander of al afgebroken dictaat', () => {
    const verwerkt: Weergave = { type: 'verwerkt', taak: 3 };
    expect(opUitslag(verwerkt, uitslag(2))).toBe(verwerkt);
    const geannuleerd: Weergave = { type: 'geannuleerd', taak: 3 };
    expect(opUitslag(geannuleerd, uitslag(3))).toBe(geannuleerd);
  });

  it('toont een fout met melding en vervolg', () => {
    const w = opGebeurtenis(
      { type: 'luistert', taak: 4, sinds: 0 },
      gebeurtenis(4, { type: 'fout', melding: 'Mondig hoort niets.', vervolg: 'geen' }),
      9
    );
    expect(w).toEqual({ type: 'fout', taak: 4, melding: 'Mondig hoort niets.', vervolg: 'geen' });
  });

  it('toont een fout van een dictaat dat nooit begon (zonder luistert)', () => {
    const w = opGebeurtenis(
      RUST,
      gebeurtenis(1 << 30, { type: 'fout', melding: 'Geen model.', vervolg: 'opnieuw_downloaden' }),
      0
    );
    expect(w.type).toBe('fout');
  });

  it('laat een lopend dictaat niet los voor een ander dictaat', () => {
    const luistert: Weergave = { type: 'luistert', taak: 5, sinds: 0 };
    expect(opGebeurtenis(luistert, gebeurtenis(6, { type: 'geannuleerd' }), 1)).toBe(luistert);
  });

  it('begint een nieuw dictaat na een afgelopen dictaat', () => {
    const klaar: Weergave = { type: 'klaar', taak: 5, uitslag: uitslag(5) };
    const w = opGebeurtenis(klaar, gebeurtenis(6, { type: 'luistert' }), 70);
    expect(w).toEqual({ type: 'luistert', taak: 6, sinds: 70 });
  });

  it('negeert downloads en andere soorten taken', () => {
    expect(opGebeurtenis(RUST, gebeurtenis(1, { type: 'luistert' }, 'download'), 0)).toBe(RUST);
    expect(opGebeurtenis(RUST, gebeurtenis(1, { type: 'luistert' }, 'bestand'), 0)).toBe(RUST);
  });

  it('annuleren geeft geannuleerd', () => {
    const w = opGebeurtenis(
      { type: 'luistert', taak: 7, sinds: 0 },
      gebeurtenis(7, { type: 'geannuleerd' }),
      3
    );
    expect(w).toEqual({ type: 'geannuleerd', taak: 7 });
    expect(isActief(w)).toBe(false);
  });
});

describe('isActief', () => {
  it('is alleen waar tijdens luisteren en verwerken', () => {
    expect(isActief(RUST)).toBe(false);
    expect(isActief({ type: 'luistert', taak: 1, sinds: 0 })).toBe(true);
    expect(isActief({ type: 'verwerkt', taak: 1 })).toBe(true);
    expect(isActief({ type: 'fout', taak: 1, melding: 'x', vervolg: 'geen' })).toBe(false);
  });
});

describe('bijnaVol', () => {
  it('waarschuwt vanaf negen minuten', () => {
    expect(bijnaVol(1000, 1000 + WAARSCHUWING_NA_MS - 1)).toBe(false);
    expect(bijnaVol(1000, 1000 + WAARSCHUWING_NA_MS)).toBe(true);
  });
});

describe('formatDuur', () => {
  it('schrijft minuten en seconden', () => {
    expect(formatDuur(0)).toBe('0:00');
    expect(formatDuur(7400)).toBe('0:07');
    expect(formatDuur(603_000)).toBe('10:03');
    expect(formatDuur(-5)).toBe('0:00');
    expect(formatDuur(756_000)).toBe('12:36');
    expect(formatDuur(3_760_000)).toBe('1:02:40');
  });
});

describe('klokTijd', () => {
  it('schrijft uren en minuten in de lokale tijd', () => {
    expect(klokTijd(new Date(2026, 10, 2, 14, 2).getTime(), 'nl')).toBe('14:02');
    expect(klokTijd(new Date(2026, 10, 2, 9, 5).getTime(), 'nl')).toBe('09:05');
  });
});

describe('stripToestand', () => {
  const klaar: Weergave = { type: 'klaar', taak: 1, uitslag: uitslag(1) };
  const fout: Weergave = { type: 'fout', taak: 1, melding: 'x', vervolg: 'geen' };

  it('toont een lopend dictaat altijd', () => {
    expect(stripToestand({ type: 'luistert', taak: 1, sinds: 0 }, true)).toBe('luistert');
    expect(stripToestand({ type: 'verwerkt', taak: 1 }, false)).toBe('verwerkt');
  });

  it('toont klaar en fout tot het startscherm is gezien', () => {
    expect(stripToestand(klaar, false)).toBe('klaar');
    expect(stripToestand(klaar, true)).toBeNull();
    expect(stripToestand(fout, false)).toBe('fout');
    expect(stripToestand(fout, true)).toBeNull();
  });

  it('toont niets in rust of na annuleren', () => {
    expect(stripToestand(RUST, false)).toBeNull();
    expect(stripToestand({ type: 'geannuleerd', taak: 1 }, false)).toBeNull();
  });
});

describe('plakHoortBij', () => {
  const klaar: Weergave = { type: 'klaar', taak: 4, uitslag: uitslag(4) };

  it('hoort bij het klare dictaat met hetzelfde id', () => {
    expect(plakHoortBij(klaar, { taak: 4, uitkomst: 'geplakt', vorigKlembord: 'bewaard' })).toBe(
      true
    );
  });

  it('een late uitslag van een vorig dictaat telt niet', () => {
    expect(plakHoortBij(klaar, { taak: 3, uitkomst: 'mislukt', vorigKlembord: 'bewaard' })).toBe(
      false
    );
  });

  it('tijdens een nieuw dictaat telt niets', () => {
    const luistert: Weergave = { type: 'luistert', taak: 5, sinds: 0 };
    expect(plakHoortBij(luistert, { taak: 5, uitkomst: 'geplakt', vorigKlembord: 'bewaard' })).toBe(
      false
    );
    expect(plakHoortBij(RUST, { taak: 5, uitkomst: 'geplakt', vorigKlembord: 'bewaard' })).toBe(
      false
    );
  });
});

describe('plakSoort', () => {
  it('geplakt is gelukt, de terugval is klembord', () => {
    expect(plakSoort('geplakt')).toBe('gelukt');
    // Getypt omdat het programma het klembord niet las: de tekst staat waar je typt.
    expect(plakSoort('getypt')).toBe('gelukt');
    expect(plakSoort('op_klembord')).toBe('klembord');
    expect(plakSoort('waarschijnlijk_geplakt')).toBe('klembord');
    // Op de Mac in een wachtwoordveld: de tekst staat op het klembord, er is niets te repareren.
    expect(plakSoort('wachtwoordveld')).toBe('klembord');
  });

  it('alles waarbij de tekst niet waar je typt staat, is een fout', () => {
    for (const uitkomst of [
      'niet_gelezen',
      'niet_geplakt',
      'beheerrechten',
      'toetsen_ingedrukt',
      'geen_venster',
      'mislukt',
      // Typen stopte of lukte niet: (de rest van) de tekst staat op het klembord.
      'typen_onderbroken',
      'typen_mislukt',
      // Geen tekstveld: niet getypt, de tekst staat op het klembord.
      'geen_tekstveld',
      // Op de Mac zonder recht op toegankelijkheid: een fout met de weg naar de instelling.
      'geen_toegang'
    ] as const) {
      expect(plakSoort(uitkomst)).toBe('fout');
    }
  });
});
