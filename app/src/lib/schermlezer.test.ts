import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { bereikteDrempel, maakMelder, MELD_PAUZE_MS } from './schermlezer';

describe('bereikteDrempel', () => {
  it('meldt precies bij 25, 50, 75 en 100 procent', () => {
    expect(bereikteDrempel(24, 25)).toBe(25);
    expect(bereikteDrempel(49, 50)).toBe(50);
    expect(bereikteDrempel(74, 75)).toBe(75);
    expect(bereikteDrempel(99, 100)).toBe(100);
  });

  it('zwijgt tussen de drempels', () => {
    for (let procent = 1; procent < 100; procent++) {
      if (procent % 25 === 0) continue;
      expect(bereikteDrempel(procent - 1, procent), `${String(procent)} procent`).toBeNull();
    }
    expect(bereikteDrempel(25, 26)).toBeNull();
    expect(bereikteDrempel(100, 100)).toBeNull();
  });

  it('meldt een drempel maar één keer', () => {
    expect(bereikteDrempel(24, 25)).toBe(25);
    expect(bereikteDrempel(25, 25)).toBeNull();
  });

  it('meldt bij een sprong alleen de hoogste gepasseerde drempel', () => {
    expect(bereikteDrempel(40, 60)).toBe(50);
    expect(bereikteDrempel(0, 100)).toBe(100);
    expect(bereikteDrempel(10, 80)).toBe(75);
  });

  it('meldt niets als er terug of stil gezet wordt', () => {
    expect(bereikteDrempel(60, 60)).toBeNull();
    expect(bereikteDrempel(60, 40)).toBeNull();
  });

  it('meldt niets over drempels die al voorbij waren toen het scherm openging', () => {
    // Een hervatte download bij 60 procent: begin met vorig = 60.
    expect(bereikteDrempel(60, 61)).toBeNull();
    expect(bereikteDrempel(74, 75)).toBe(75);
  });
});

describe('maakMelder', () => {
  it('maakt de regio eerst leeg, zodat dezelfde zin twee keer achter elkaar toch verandert', async () => {
    const geschreven: string[] = [];
    const melder = maakMelder(
      (tekst) => geschreven.push(tekst),
      () => Promise.resolve()
    );
    melder('20 dictaten erbij getoond');
    await Promise.resolve();
    melder('20 dictaten erbij getoond');
    await Promise.resolve();
    expect(geschreven).toEqual(['', '20 dictaten erbij getoond', '', '20 dictaten erbij getoond']);
  });

  it('zet de zin pas nadat er gewacht is', async () => {
    const geschreven: string[] = [];
    let klaar: () => void = () => undefined;
    const melder = maakMelder(
      (tekst) => geschreven.push(tekst),
      () =>
        new Promise<void>((los) => {
          klaar = los;
        })
    );
    melder('Gekopieerd');
    expect(geschreven).toEqual(['']);
    klaar();
    await Promise.resolve();
    await Promise.resolve();
    expect(geschreven).toEqual(['', 'Gekopieerd']);
  });
});

describe('maakMelder met een echte pauze', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('zet de zin pas na ongeveer 100 ms', () => {
    const geschreven: string[] = [];
    const melder = maakMelder((tekst) => geschreven.push(tekst));
    melder('Gekopieerd');
    expect(geschreven).toEqual(['']);
    vi.advanceTimersByTime(MELD_PAUZE_MS - 1);
    expect(geschreven).toEqual(['']);
    vi.advanceTimersByTime(1);
    return Promise.resolve().then(() => {
      expect(geschreven).toEqual(['', 'Gekopieerd']);
    });
  });

  it('laat een oudere zin die nog wacht vervallen als er een nieuwere komt', async () => {
    const geschreven: string[] = [];
    const melder = maakMelder((tekst) => geschreven.push(tekst));
    melder('eerste');
    melder('tweede');
    await vi.advanceTimersByTimeAsync(MELD_PAUZE_MS);
    expect(geschreven).toEqual(['', '', 'tweede']);
  });
});
