import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { ModellenOverzicht, ModelStand } from '#lib/modellen.js';
import { modellenStand } from '#lib/modellenstand.svelte.js';
import InstellingSprekers from './InstellingSprekers.svelte';

function overzicht(status: ModelStand): ModellenOverzicht {
  return {
    modellen: [
      {
        id: 'sortformer-4spk-v2.1-q8',
        naam: 'Sortformer',
        soort: 'sprekers',
        rol: null,
        nauwkeurigheid: 1,
        snelheid: 1,
        talen: [],
        status,
        grootteBytes: 139_310_336,
        aanwezigBytes: status === 'compleet' ? 139_310_336 : 0,
        tekortBytes: 0,
        nietVoor: ['dicteren', 'bestanden'],
        inGebruikVoor: [],
        kanWeg: false,
        nieuw: false,
        bron: '',
        daarnaBijHervatten: []
      }
    ],
    dicteren: 'x',
    bestanden: 'x',
    downloadt: null,
    totaalBytes: 0,
    vrijBytes: null
  };
}

// Op Bestanden staat "Sprekers herkennen" niet meer als volle rij bovenaan, maar als compacte
// regel met schakelaar onder "Bestanden kiezen" in het sleepvlak (gebruikerstest na N-C).

describe('Sprekers herkennen compact', () => {
  it('is één regel met schakelaar, naam en hulptekst met tot 4 sprekers', () => {
    const html = render(InstellingSprekers, { props: { variant: 'compact' } }).body;
    expect(html).toContain('role="switch"');
    expect(html).toContain('Sprekers herkennen');
    expect(html).toContain('tot 4 sprekers');
    // Geen rij uit Instellingen: geen RijKop met de lange uitleg.
    expect(html).not.toContain('Heeft een apart model nodig');
  });

  it('noemt de grootte alleen zolang het sprekermodel nog gedownload moet worden', () => {
    const compact = (): string =>
      render(InstellingSprekers, { props: { variant: 'compact' } }).body;
    // Zonder overzicht weet Mondig het niet: geen downloadtekst.
    expect(compact()).not.toMatch(/MB/);
    modellenStand.overzicht = overzicht('ontbreekt');
    expect(compact()).toMatch(/Eerst .+ MB downloaden/);
    modellenStand.overzicht = overzicht('compleet');
    expect(compact()).not.toMatch(/MB/);
    modellenStand.overzicht = null;
  });

  it('staat op Bestanden onder Bestanden kiezen in het sleepvlak, niet bovenaan', () => {
    const bron = readFileSync(join(__dirname, '../../routes/bestanden/+page.svelte'), 'utf-8');
    expect(bron).not.toContain('<div class={vlak}><InstellingSprekers />');
    expect(bron).toContain('<InstellingSprekers variant="compact" />');
    const kiezen = bron.indexOf('{t().bestandenKiezen}');
    const regel = bron.indexOf('{@render sprekersRegel()}');
    expect(kiezen).toBeGreaterThan(0);
    expect(regel).toBeGreaterThan(kiezen);
  });
});
