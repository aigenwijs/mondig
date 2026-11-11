import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import MetaRegel from './MetaRegel.svelte';

// Het middenpunt hangt aan een niet-brekende spatie, zodat het bij het stuk ervoor blijft.
const NBSP = String.fromCharCode(160);
const nu = new Date(2026, 10, 10, 12, 0).getTime();
const okt30 = new Date(2026, 9, 30, 23, 14).getTime();

/** De tekst zonder tags, met witruimte en niet-brekende spaties genormaliseerd. */
function tekst(props: Record<string, unknown>): string {
  const html = render(MetaRegel, { props: { nu, ...props } }).body;
  return html
    .replace(/<!--.*?-->/g, '')
    .replace(/<svg.*?<\/svg>/g, '')
    .replace(/<\/span><span class="whitespace-nowrap"/g, '</span> <span class="whitespace-nowrap"')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;|\u00a0/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('MetaRegel', () => {
  it('zet soort, datum, duur en plek achter elkaar met middenpunten (ontwerpsysteem §4)', () => {
    expect(
      tekst({
        soort: 'Online vergadering',
        datumMs: okt30,
        duurMs: 23 * 60_000 + 14_000,
        plek: 'op deze computer'
      })
    ).toBe('Online vergadering · 30 okt · 23 min · op deze computer');
  });

  it('schrijft de duur nooit als kloktijd en laat het jaar weg dit jaar', () => {
    const uit = tekst({ datumMs: okt30, duurMs: 45_000 });
    expect(uit).toBe('30 okt · 45 s');
    expect(uit).not.toContain(':');
    expect(uit).not.toContain('2026');
  });

  it('zet het jaar erbij als de datum niet van dit jaar is', () => {
    expect(tekst({ datumMs: new Date(2025, 9, 30).getTime() })).toBe('30 okt 2025');
  });

  it('laat weg wat ontbreekt, zonder los middenpunt', () => {
    expect(tekst({ duurMs: 38 * 60_000, plek: 'op deze computer' })).toBe(
      '38 min · op deze computer'
    );
    expect(tekst({ soort: 'Alleen ik' })).toBe('Alleen ik');
  });

  it('zet het middenpunt achter het stuk ervoor, zodat een afgebroken regel er niet mee begint', () => {
    const html = render(MetaRegel, {
      props: { nu, soort: 'In de ruimte', datumMs: okt30, duurMs: 60_000 }
    }).body;
    // Elk stuk behalve het laatste draagt zijn eigen punt, binnen dezelfde niet-brekende span.
    const stukken = html.split('<span class="whitespace-nowrap"').slice(1);
    expect(stukken).toHaveLength(3);
    expect(stukken[0]).toContain(`${NBSP}·`);
    expect(stukken[1]).toContain(`${NBSP}·`);
    expect(stukken[2]).not.toContain('·');
    // Het punt staat nooit los tussen twee stukken.
    expect(html).not.toContain(`</span><span aria-hidden="true">${NBSP}·`);
    // Het punt is voor de schermlezer verborgen.
    expect(html).toContain(`<span aria-hidden="true">${NBSP}·</span>`);
  });

  it('zet bij de plek een tooltip en tekst voor de schermlezer', () => {
    const html = render(MetaRegel, {
      props: {
        nu,
        plek: 'op deze computer',
        plekTitel: 'Uitgeschreven met Snel',
        plekSr: 'Uitgeschreven met Snel',
        plekIcoon: true
      }
    }).body;
    expect(html).toContain('title="Uitgeschreven met Snel"');
    expect(html).toContain('class="sr-only">, Uitgeschreven met Snel');
    expect(html).toContain('<svg');
  });
});
