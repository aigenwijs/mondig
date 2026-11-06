import { describe, expect, it } from 'vitest';
import { zoekOnderdelen, type Onderdeel } from './vermeldingen';

const maak = (naam: string, licentie: string): Onderdeel => ({
  naam,
  versie: '1.0.0',
  bron: 'rust',
  licentie,
  teksten: [],
  notice: null,
  url: null
});

const lijst = [maak('serde', 'MIT OR Apache-2.0'), maak('svelte', 'MIT'), maak('sora', 'OFL-1.1')];

describe('zoekOnderdelen', () => {
  it('geeft alles bij een lege zoekterm', () => {
    expect(zoekOnderdelen(lijst, '  ')).toHaveLength(3);
  });

  it('zoekt op naam zonder letop hoofdletters', () => {
    expect(zoekOnderdelen(lijst, 'SVE').map((o) => o.naam)).toEqual(['svelte']);
  });

  it('zoekt ook op licentie', () => {
    expect(zoekOnderdelen(lijst, 'ofl').map((o) => o.naam)).toEqual(['sora']);
  });

  it('geeft niets als niets past', () => {
    expect(zoekOnderdelen(lijst, 'bestaatniet')).toEqual([]);
  });
});
