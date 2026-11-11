import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import type { ModellenOverzicht, ModelStand } from '#lib/modellen.js';
import { modellenStand } from '#lib/modellenstand.svelte.js';
import SprekersKaart from './SprekersKaart.svelte';

// De kaart "Sprekers herkennen" onder "Wat Mondig gebruikt" (UI-check punt 26): zolang het
// sprekermodel er niet is, staat er een knop "Downloaden (139 MB)" en geen schakelaar.

function overzicht(status: ModelStand, aanwezigBytes = 0): ModellenOverzicht {
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
        aanwezigBytes: status === 'compleet' ? 139_310_336 : aanwezigBytes,
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

const kaart = (): string => render(SprekersKaart).body;

describe('de kaart Sprekers herkennen', () => {
  afterEach(() => {
    modellenStand.overzicht = null;
  });

  it('toont zonder sprekermodel een downloadknop met de grootte en geen schakelaar', () => {
    modellenStand.overzicht = overzicht('ontbreekt');
    const uit = kaart();
    expect(uit).toContain('Downloaden (139 MB)');
    expect(uit).not.toContain('role="switch"');
    expect(uit).toContain('Nog niet gedownload.');
  });

  it('rekent met wat er al binnen is bij een half gedownload model', () => {
    modellenStand.overzicht = overzicht('gedeeltelijk', 69_655_168);
    expect(kaart()).toContain('Downloaden (70 MB)');
  });

  it('toont na de download de schakelaar en geen knop', () => {
    modellenStand.overzicht = overzicht('compleet');
    const uit = kaart();
    expect(uit).toContain('role="switch"');
    expect(uit).not.toContain('Downloaden (');
    expect(uit).toContain('Staat op deze computer.');
  });
});
