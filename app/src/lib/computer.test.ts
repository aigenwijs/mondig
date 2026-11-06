import { describe, expect, it } from 'vitest';
import {
  computerZin,
  geheugenTekst,
  standaardAdvies,
  videokaartAdviesTekst,
  type Advies
} from './computer';
import { talen } from './i18n/index.svelte';

const nl = talen.nl;
const en = talen.en;

function advies(stand: Advies['videokaart']['stand'], keer: number | null = null): Advies {
  return {
    geheugen: 'ruim',
    dicteren: 'parakeet-ultra-q8',
    bestanden: 'canary-1b-v2-q8',
    bestandenApart: true,
    krap: false,
    tekortDicterenBytes: 0,
    tekortBeideBytes: 0,
    videokaart: { stand, keerSneller: keer }
  };
}

describe('de computer in gewone taal', () => {
  it('rondt het geheugen af op hele GB (een laptop van 8 GB meldt 7,4)', () => {
    expect(geheugenTekst(7.4 * 1024 ** 3)).toBe('8 GB');
    expect(geheugenTekst(16 * 1024 ** 3)).toBe('16 GB');
    expect(geheugenTekst(15.4 * 1024 ** 3)).toBe('16 GB');
    expect(geheugenTekst(3.8 * 1024 ** 3)).toBe('4 GB');
    expect(geheugenTekst(null)).toBeNull();
  });

  it('beschrijft geheugen, kernen en een losse kaart', () => {
    const zin = computerZin(
      {
        geheugenBytes: 16 * 1024 ** 3,
        kernen: 8,
        vrijBytes: null,
        videokaarten: [{ naam: 'NVIDIA GeForce RTX 4090', soort: 'los' }]
      },
      nl
    );
    expect(zin).toBe('16 GB werkgeheugen, 8 processorkernen, videokaart NVIDIA GeForce RTX 4090');
  });

  it('zegt dat er geen losse kaart is, maar zwijgt als zoeken niet lukte', () => {
    const basis = { geheugenBytes: null, kernen: null, vrijBytes: null };
    expect(computerZin({ ...basis, videokaarten: [] }, nl)).toBe('geen losse videokaart');
    expect(computerZin({ ...basis, videokaarten: null }, nl)).toBe('');
  });
});

describe('het advies over de videokaart', () => {
  it('raadt de kaart alleen aan na een proef die winst laat zien', () => {
    expect(videokaartAdviesTekst(advies('aanzetten', 9.2), nl, 'nl')).toContain('9,2 keer sneller');
    expect(videokaartAdviesTekst(advies('aanzetten', 9.2), en, 'en')).toContain('9.2 times');
    // Zonder proef een uitnodiging om te testen, nooit een aanbeveling.
    expect(videokaartAdviesTekst(advies('testen'), nl, 'nl')).not.toContain('Gebruik hem');
    expect(videokaartAdviesTekst(advies('laten'), nl, 'nl')).toContain('processor');
    expect(videokaartAdviesTekst(advies('faalde'), nl, 'nl')).toContain('fout');
    expect(videokaartAdviesTekst(advies('geen'), nl, 'nl')).toContain('processor');
    expect(videokaartAdviesTekst(advies('ingebouwd'), nl, 'nl')).toContain('ingebouwde');
  });
});

describe('zonder metingen', () => {
  it('geeft het standaardmodel voor beide taken en raadt de videokaart niet aan', () => {
    const info = standaardAdvies();
    expect(info.advies.dicteren).toBe('parakeet-ultra-q8');
    expect(info.advies.bestanden).toBe(info.advies.dicteren);
    expect(info.advies.bestandenApart).toBe(false);
    expect(info.advies.tekortDicterenBytes).toBe(0);
    expect(info.advies.videokaart.stand).toBe('geen');
  });
});
