import { afterEach, describe, expect, it } from 'vitest';
import { huidigeTaal, t, talen, zetTaal } from './index.svelte';

describe('vertalingen', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('nl en en hebben dezelfde sleutels', () => {
    expect(Object.keys(talen.en).sort()).toEqual(Object.keys(talen.nl).sort());
  });

  it('nl en en hebben voor elke sleutel hetzelfde soort bericht', () => {
    for (const sleutel of Object.keys(talen.nl) as (keyof typeof talen.nl)[]) {
      expect(typeof talen.en[sleutel], sleutel).toBe(typeof talen.nl[sleutel]);
    }
  });

  it('geen enkel tekstbericht is leeg', () => {
    for (const [taal, berichten] of Object.entries(talen)) {
      for (const [sleutel, waarde] of Object.entries(berichten)) {
        if (typeof waarde === 'string') {
          expect(waarde.trim(), `${taal}.${sleutel}`).not.toBe('');
        }
      }
    }
  });

  it('kiest het meervoud bij een functiebericht', () => {
    expect(t().aantalBestanden(0)).toBe('0 bestanden');
    expect(t().aantalBestanden(1)).toBe('1 bestand');
    expect(t().aantalBestanden(2)).toBe('2 bestanden');
    zetTaal('en');
    expect(t().aantalBestanden(1)).toBe('1 file');
    expect(t().aantalBestanden(3)).toBe('3 files');
  });

  it('noemt een standaardapparaat met de naam eerst en "(standaard)" erachter', () => {
    expect(t().apparaatStandaard('Headset Mic (AKG)')).toBe('Headset Mic (AKG) (standaard)');
    expect(t().apparaatStandaard(null)).toBe('Standaard van Windows');
    zetTaal('en');
    expect(t().apparaatStandaard('Speakers')).toBe('Speakers (default)');
  });

  it('staat standaard op nl en wisselt met zetTaal', () => {
    expect(huidigeTaal()).toBe('nl');
    zetTaal('en');
    expect(huidigeTaal()).toBe('en');
    expect(t().homeDictate).toBe('Dictate');
  });
});
