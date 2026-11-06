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

function html(b: BeurtInfo, extra: { kanAanSpreker?: boolean } = {}): string {
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
