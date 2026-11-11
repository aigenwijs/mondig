import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import { zetTaal } from '#lib';
import LijstMeer from './LijstMeer.svelte';

// De voet van een lijst met maat (ontwerpsysteem §5). De klik en de focus na het laden doen
// effecten en een DOM; die zijn in de mock (`?mock=dictatenweek`) met de hand gecontroleerd.
// Hier de zichtbare regels: wanneer de knop en de telling er zijn.

const stil = () => Promise.resolve();

function html(props: { getoond: number; totaal: number; id?: string; label?: string }): string {
  return render(LijstMeer, { props: { meer: stil, rij: () => null, ...props } }).body;
}

describe('LijstMeer', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('toont Meer tonen links en de telling rechts als er meer is dan zichtbaar', () => {
    const uit = html({ getoond: 10, totaal: 37 });
    expect(uit).toContain('Meer tonen');
    expect(uit).toContain('10 van 37');
    expect(uit.indexOf('Meer tonen')).toBeLessThan(uit.indexOf('10 van 37'));
    // De telling staat rechts: een marge die haar naar het eind van de rij duwt.
    expect(uit).toMatch(/class="[^"]*\bml-auto\b[^"]*"[^>]*>10 van 37/);
  });

  it('toont niets als alles zichtbaar is', () => {
    for (const uit of [html({ getoond: 37, totaal: 37 }), html({ getoond: 0, totaal: 0 })]) {
      expect(uit).not.toContain('Meer tonen');
      expect(uit).not.toMatch(/\d+ van \d+/);
      expect(uit).not.toContain('<button');
    }
  });

  it('geeft de knop het id van de ouder, en een vast id als die niets geeft', () => {
    expect(html({ getoond: 3, totaal: 9, id: 'dictaten-meer' })).toContain('id="dictaten-meer"');
    expect(html({ getoond: 3, totaal: 9 })).toContain('id="lijst-meer"');
  });

  it('toont met een label dat in plaats van "Meer tonen" en laat de telling weg', () => {
    const uit = html({ getoond: 3, totaal: 6, label: 'Alle 6 tonen' });
    expect(uit).toContain('Alle 6 tonen');
    expect(uit).not.toContain('Meer tonen');
    expect(uit).not.toMatch(/\d+ van \d+/);
    expect(uit).not.toContain('ml-auto');
  });

  it('toont met een label niets als alles zichtbaar is', () => {
    const uit = html({ getoond: 6, totaal: 6, label: 'Alle 6 tonen' });
    expect(uit).not.toContain('Alle 6 tonen');
    expect(uit).not.toContain('<button');
  });

  it('volgt de taal', () => {
    zetTaal('en');
    const uit = html({ getoond: 5, totaal: 12 });
    expect(uit).toContain('Show more');
    expect(uit).toContain('5 of 12');
  });
});
