import { describe, expect, it, vi } from 'vitest';
import type { Meter, MeterEinde, MeterOpties } from './meter';
import { MICTEST_STILTE_MS, maakMicTest, type MicTestOmgeving, type MicTestStand } from './mictest';
import type { Foutmelding } from './taak';

// Een nep-klok en een nep-meter: wat de test aan en uit zet is te volgen zonder app of browser.
function opzet(apparaat: string | null = null) {
  let nu = 0;
  const timers = new Map<number, { op: number; functie: () => void }>();
  let teller = 0;
  const stop = vi.fn();
  const starts: MeterOpties[] = [];
  const omgeving: MicTestOmgeving = {
    startMeter: vi.fn((opties: MeterOpties): Meter => {
      starts.push(opties);
      return { stop };
    }),
    setTimeout: (functie, ms) => {
      teller += 1;
      timers.set(teller, { op: nu + ms, functie });
      return teller;
    },
    clearTimeout: (id) => {
      timers.delete(id as number);
    }
  };
  const standen: MicTestStand[] = [];
  const test = maakMicTest(
    () => apparaat,
    (s) => standen.push(s),
    omgeving
  );
  return {
    test,
    omgeving,
    stop,
    starts,
    standen,
    laatste: () => standen.at(-1),
    /** Laat de klok `ms` verder lopen. */
    tik(ms: number) {
      nu += ms;
      for (const [id, t] of [...timers]) {
        if (t.op <= nu) {
          timers.delete(id);
          t.functie();
        }
      }
    }
  };
}

describe('microfoon testen', () => {
  it('zet de microfoon niet aan bij het maken (openen van Dicteren)', () => {
    const o = opzet();
    expect(o.omgeving.startMeter).not.toHaveBeenCalled();
    expect(o.standen).toEqual([]);
  });

  it('zet de microfoon aan na de klik, op de gekozen microfoon', () => {
    const o = opzet('mic-7');
    o.test.begin();
    expect(o.omgeving.startMeter).toHaveBeenCalledTimes(1);
    expect(o.starts[0]?.bron).toBe('microfoon');
    expect(o.starts[0]?.apparaat).toBe('mic-7');
    expect(o.laatste()?.aan).toBe(true);
  });

  it('start geen tweede meter bij een tweede klik', () => {
    const o = opzet();
    o.test.begin();
    o.test.begin();
    expect(o.omgeving.startMeter).toHaveBeenCalledTimes(1);
  });

  it('toont twintig streepjes en laat een stille microfoon leeg', () => {
    const o = opzet();
    o.test.begin();
    o.starts[0]?.bijNiveau(1);
    expect(o.laatste()?.streepjes).toBe(20);
    o.starts[0]?.bijNiveau(0.5);
    expect(o.laatste()?.streepjes).toBe(10);
    o.starts[0]?.bijNiveau(0);
    expect(o.laatste()?.streepjes).toBe(0);
  });

  it('toont tien streepjes als de meter er tien heeft (Instellingen)', () => {
    const start: MeterOpties[] = [];
    const stand: MicTestStand[] = [];
    const omgeving: MicTestOmgeving = {
      startMeter: (opties) => {
        start.push(opties);
        return { stop: vi.fn() };
      },
      setTimeout: () => 1,
      clearTimeout: () => undefined
    };
    const test = maakMicTest(
      () => null,
      (s) => stand.push(s),
      omgeving,
      10
    );
    test.begin();
    start[0]?.bijNiveau(1);
    expect(stand.at(-1)?.streepjes).toBe(10);
    start[0]?.bijNiveau(0.5);
    expect(stand.at(-1)?.streepjes).toBe(5);
  });

  it('zet alles uit bij Klaar', () => {
    const o = opzet();
    o.test.begin();
    o.test.klaar();
    expect(o.stop).toHaveBeenCalledTimes(1);
    expect(o.laatste()).toMatchObject({ aan: false, streepjes: 0, einde: 'klaar' });
  });

  it('zet alles uit na 15 seconden stilte', () => {
    const o = opzet();
    o.test.begin();
    o.tik(MICTEST_STILTE_MS - 1);
    expect(o.stop).not.toHaveBeenCalled();
    o.tik(1);
    expect(o.stop).toHaveBeenCalledTimes(1);
    expect(o.laatste()).toMatchObject({ aan: false, einde: 'stilte' });
  });

  it('telt de stilte opnieuw zodra de microfoon iets hoort', () => {
    const o = opzet();
    o.test.begin();
    o.tik(10_000);
    o.starts[0]?.bijNiveau(0.4);
    o.tik(10_000);
    // Tien seconden na het geluid: nog aan.
    expect(o.stop).not.toHaveBeenCalled();
    o.tik(5_000);
    expect(o.stop).toHaveBeenCalledTimes(1);
  });

  it('telt ruis onder de stilte-drempel niet als geluid', () => {
    const o = opzet();
    o.test.begin();
    o.tik(10_000);
    o.starts[0]?.bijNiveau(0.02);
    o.tik(5_000);
    expect(o.stop).toHaveBeenCalledTimes(1);
  });

  it('zet alles uit als het scherm verdwijnt en meldt daarna niets meer', () => {
    const o = opzet();
    o.test.begin();
    const aantal = o.standen.length;
    o.test.sluit();
    expect(o.stop).toHaveBeenCalledTimes(1);
    o.tik(MICTEST_STILTE_MS * 2);
    expect(o.standen.length).toBe(aantal);
  });

  it('start na het verlaten van het scherm niet meer', () => {
    const o = opzet();
    o.test.sluit();
    o.test.begin();
    expect(o.omgeving.startMeter).not.toHaveBeenCalled();
  });

  it('kan opnieuw getest worden na Klaar', () => {
    const o = opzet();
    o.test.begin();
    o.test.klaar();
    o.test.begin();
    expect(o.omgeving.startMeter).toHaveBeenCalledTimes(2);
    expect(o.laatste()?.aan).toBe(true);
  });

  it('toont een bezette microfoon als melding en houdt de meter leeg', () => {
    const o = opzet();
    o.test.begin();
    const melding: Foutmelding = {
      melding: 'Een ander programma gebruikt de microfoon.',
      vervolg: 'opnieuw_proberen'
    };
    o.starts[0]?.bijFout?.(melding);
    expect(o.laatste()).toMatchObject({ aan: true, streepjes: 0, fout: melding });
    o.test.klaar();
    expect(o.laatste()?.aan).toBe(false);
  });

  it('stopt de meter echt bij een fout, zodat de microfoon niet stilletjes terugkomt', () => {
    const o = opzet();
    o.test.begin();
    o.starts[0]?.bijFout?.({ melding: 'Bezet.', vervolg: 'opnieuw_proberen' });
    // De meter is gestopt (en daarmee zijn zichtbaarheidsluisteraar weg), de stiltetimer ook.
    expect(o.stop).toHaveBeenCalledTimes(1);
    o.tik(MICTEST_STILTE_MS * 2);
    expect(o.laatste()).toMatchObject({ aan: true, fout: { melding: 'Bezet.' }, einde: null });
    // Een nieuwe klik start opnieuw een verse meter.
    o.test.klaar();
    o.test.begin();
    expect(o.omgeving.startMeter).toHaveBeenCalledTimes(2);
  });

  it('stopt zonder melding als een dictaat of opname voorrang krijgt', () => {
    const o = opzet();
    o.test.begin();
    const einde: MeterEinde = { bron: 'microfoon', reden: 'voorrang', fout: null };
    o.starts[0]?.bijEinde?.(einde);
    expect(o.laatste()).toMatchObject({ aan: false, einde: 'voorrang', fout: null });
  });
});
