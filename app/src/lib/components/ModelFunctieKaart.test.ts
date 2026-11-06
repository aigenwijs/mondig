import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import { controleStand } from '#lib/controlestand.svelte.js';
import type { ModelRegel } from '#lib/modellen.js';
import { modellenStand } from '#lib/modellenstand.svelte.js';
import ModelFunctieKaart from './ModelFunctieKaart.svelte';

// De functiekaarten op Modellen (gebruikerstest na N-C): de rol als hoofdtekst, de naam klein, en
// "Past bij deze computer" alleen als de laatste controle dat model voor deze functie aanraadt.

function regel(id: string, naam: string, rol: ModelRegel['rol']): ModelRegel {
  return {
    id,
    naam,
    soort: 'spraak',
    rol,
    nauwkeurigheid: 3,
    snelheid: 3,
    talen: ['nl'],
    status: 'compleet',
    grootteBytes: 1,
    aanwezigBytes: 1,
    tekortBytes: 0,
    nietVoor: [],
    inGebruikVoor: [],
    kanWeg: false,
    nieuw: false,
    bron: '',
    daarnaBijHervatten: []
  };
}

function zetOverzicht(): void {
  modellenStand.overzicht = {
    modellen: [
      regel('parakeet-ultra-q8', 'Parakeet Ultra', 'snel'),
      regel('canary-1b-v2-q8', 'Canary 1b-v2', 'nauwkeurig')
    ],
    dicteren: 'parakeet-ultra-q8',
    bestanden: 'canary-1b-v2-q8',
    downloadt: null,
    totaalBytes: 0,
    vrijBytes: null
  };
}

describe('de functiekaart op Modellen', () => {
  afterEach(() => {
    modellenStand.overzicht = null;
    controleStand.controle = null;
  });

  it('toont de rol als hoofdtekst en de naam klein erbij', () => {
    zetOverzicht();
    const html = render(ModelFunctieKaart, { props: { gebruik: 'bestanden' } }).body;
    expect(html).toContain('Opnames en bestanden');
    expect(html).toContain('Nauwkeurig model');
    expect(html).toContain('Canary');
    expect(html).toContain('Hier telt nauwkeurigheid.');
  });

  it('zegt "Past bij deze computer" bij het model uit de laatste controle', () => {
    zetOverzicht();
    controleStand.controle = {
      dicteren: 'parakeet-ultra-q8',
      bestanden: 'canary-1b-v2-q8',
      videokaart: 'laten',
      datumMs: 0
    };
    expect(render(ModelFunctieKaart, { props: { gebruik: 'dicteren' } }).body).toContain(
      'Past bij deze computer'
    );
  });

  it('zegt het niet als de controle een ander model aanraadt', () => {
    zetOverzicht();
    controleStand.controle = {
      dicteren: 'parakeet-ultra-q8',
      bestanden: 'parakeet-ultra-q8',
      videokaart: 'geen',
      datumMs: 0
    };
    expect(render(ModelFunctieKaart, { props: { gebruik: 'bestanden' } }).body).not.toContain(
      'Past bij deze computer'
    );
  });

  it('zegt het nergens zonder controle', () => {
    zetOverzicht();
    for (const gebruik of ['dicteren', 'bestanden'] as const) {
      expect(render(ModelFunctieKaart, { props: { gebruik } }).body).not.toContain(
        'Past bij deze computer'
      );
    }
  });

  it('biedt bij Wijzigen alleen de modellen die bij de functie passen', () => {
    // Het menu staat in een portal en is in een serverrender dicht: de keuzes komen uit
    // `passendVoor` (getest in modellen.test.ts), nergens anders vandaan.
    const bron = readFileSync(join(__dirname, 'ModelFunctieKaart.svelte'), 'utf-8');
    expect(bron).toContain('passendVoor(overzicht.modellen, gebruik)');
    expect(bron).toMatch(/\{#each keuzes as regel/);
  });
});
