import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { BeurtInfo } from '../vergadering';
import SprekerNaam from './SprekerNaam.svelte';

function beurt(deel: Partial<BeurtInfo> = {}): BeurtInfo {
  return {
    index: 3,
    kant: 'anderen',
    sprekernummer: null,
    spreker: 'Anderen',
    sprekerGetoond: deel.spreker === undefined ? 'Anderen' : deel.spreker,
    eigenNaam: false,
    startMs: 0,
    eindMs: 1000,
    tekst: 'Hallo.',
    markeringen: [],
    veranderd: false,
    ...deel
  };
}

const geef = (): Promise<string | null> => Promise.resolve(null);

function html(
  b: BeurtInfo,
  extra: { kanAanSpreker?: boolean; aanwijzen?: 'rij' | 'kolom' } = {}
): string {
  return render(SprekerNaam, { props: { beurt: b, geef, ...extra } }).body;
}

describe('het label van een beurt', () => {
  it('toont Spreker N als de sprekers herkend zijn en de spreker geen naam heeft', () => {
    const uit = html(beurt({ sprekernummer: 2, spreker: 'Spreker 2' }));
    expect(uit).toContain('Spreker 2');
    expect(uit).toContain('aria-label="Spreker 2, naam wijzigen"');
  });

  it('toont de naam die de gebruiker gaf, ook bij een herkende spreker', () => {
    const uit = html(beurt({ sprekernummer: 2, spreker: 'Marieke', eigenNaam: true }));
    expect(uit).toContain('Marieke');
    expect(uit).not.toContain('Spreker 2');
  });

  it('blijft Jij of Anderen zonder sprekernummers', () => {
    expect(html(beurt())).toContain('aria-label="Anderen, naam wijzigen"');
    expect(html(beurt({ kant: 'jij', spreker: 'Jij' }))).toContain(
      'aria-label="Jij, naam wijzigen"'
    );
  });

  it('heeft een gestippeld "+ Spreker" bij een beurt zonder spreker in een verslag met sprekers', () => {
    const zonder = html(beurt(), { kanAanSpreker: true });
    expect(zonder).toContain('+ Spreker');
    expect(zonder).toContain('border-dashed');
    // Een beurt met spreker, of een verslag zonder sprekers, heeft het niet.
    expect(
      html(beurt({ sprekernummer: 1, spreker: 'Spreker 1' }), { kanAanSpreker: true })
    ).not.toContain('+ Spreker');
    expect(html(beurt())).not.toContain('+ Spreker');
  });
});

describe('een spreker aanwijzen bij een alinea zonder spreker', () => {
  const alinea = beurt({ kant: 'jij', spreker: null, startMs: 65_000 });

  it('naast de alinea is een kleine knop met een eigen naam voor de schermlezer, geen label', () => {
    const uit = html(alinea, { kanAanSpreker: true, aanwijzen: 'rij' });
    expect(uit).toContain('aria-label="Spreker aanwijzen bij de alinea van 1:05"');
    expect(uit).toContain('title="Spreker aanwijzen"');
    // Altijd bereikbaar met het toetsenbord: een gewone knop, alleen onzichtbaar tot aanwijzen.
    expect(uit).toContain('<button');
    expect(uit).toContain('group-focus-within/rij:opacity-100');
    expect(uit).toContain('focus-visible:opacity-100');
    expect(uit).not.toContain('naam wijzigen');
    expect(uit).not.toContain('Jij');
  });

  it('in de labelkolom is het een gestippeld "+ Spreker" dat er altijd staat, zonder "Jij"', () => {
    const uit = html(alinea, { kanAanSpreker: true, aanwijzen: 'kolom' });
    expect(uit).toContain('+ Spreker');
    expect(uit).toContain('border-dashed');
    expect(uit).not.toContain('opacity-0');
    expect(uit).toContain('aria-label="Spreker aanwijzen bij de alinea van 1:05"');
    expect(uit).not.toContain('Jij');
  });

  it('is er niet zonder aanwijzen: dan blijft het label, met een merkteken voor de focus', () => {
    const uit = html(beurt({ kant: 'jij', spreker: 'Jij' }), { kanAanSpreker: true });
    expect(uit).not.toContain('Spreker aanwijzen');
    expect(uit).toContain('aria-label="Jij, naam wijzigen"');
    expect(uit).toContain('data-label-beurt="3"');
  });
});

describe('een naam geven', () => {
  it('heeft geen keuze meer voor alleen deze beurt: de naam geldt voor de hele spreker', () => {
    const uit = render(SprekerNaam, { props: { beurt: beurt(), geef } }).body;
    expect(uit).not.toContain('Alleen deze beurt');
    expect(uit).not.toContain('type="radio"');
  });
});
