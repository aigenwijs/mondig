import { render } from 'svelte/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Het paneel op Dicteren met de dictaatgeschiedenis (ADR 0021): bij Niet bewaren alleen het
// laatste dictaat uit het geheugen, anders de lijst. De commando's zijn een nep.

const kern = vi.hoisted(() => ({
  invoke: vi.fn(() => Promise.resolve(null)),
  isTauri: vi.fn(() => false)
}));
vi.mock('@tauri-apps/api/core', () => kern);

import { dictaatStand } from '#lib/dictaatstand.svelte.js';
import { dictatenStand } from '#lib/dictatenstand.svelte.js';
import DictaatPaneel from './DictaatPaneel.svelte';

const html = (): string =>
  render(DictaatPaneel, { props: { sneltoets: null, toonRust: false } }).body;

describe('Dicteren met de dictaatgeschiedenis', () => {
  beforeEach(() => {
    dictaatStand.dictaat = 'Dit is het laatste dictaat.';
    dictaatStand.dictaatOm = new Date(2026, 10, 4, 14, 2).getTime();
    dictatenStand.geladen = true;
    dictatenStand.termijn = '24u';
    dictatenStand.dictaten = [];
    dictatenStand.laatste = null;
    dictatenStand.nu = Date.now();
    dictaatStand.weergave = { type: 'rust' };
    dictaatStand.plak = null;
  });

  it('toont bij Niet bewaren het laatste dictaat met tijd, Kopiëren en de uitleg met Wijzigen', () => {
    dictatenStand.termijn = 'niet';
    const uit = html();
    expect(uit).toContain('Laatste dictaat');
    expect(uit).toContain('14:02');
    expect(uit).toContain('Dit is het laatste dictaat.');
    expect(uit).toContain('Kopiëren');
    expect(uit).toContain('Mondig bewaart je dictaten niet; dit verdwijnt als je Mondig sluit.');
    expect(uit).toMatch(
      /<a[^>]*href="[^"]*instellingen#dictaten-bewaren"[^>]*>(?:<!---->)?Wijzigen/
    );
    expect(uit).not.toContain('Je dictaten');
  });

  it('heeft een statusregio met focusdoel die ook bij Niet bewaren blijft bestaan', () => {
    for (const termijn of ['niet', '24u', 'laatste10'] as const) {
      dictatenStand.termijn = termijn;
      expect(html()).toMatch(/<div[^>]*id="dictaten-status"[^>]*role="status"[^>]*tabindex="-1"/);
    }
  });

  it('toont bij 24 uur de lijst Je dictaten in plaats van het losse laatste dictaat', () => {
    dictatenStand.dictaten = [
      {
        id: 'dictaat-1-0.json',
        tijdUnixMs: Date.now() - 60_000,
        tekst: 'Eerste regel van de lijst.'
      }
    ];
    const uit = html();
    expect(uit).toContain('Je dictaten');
    expect(uit).toContain('Eerste regel van de lijst.');
    expect(uit).not.toContain('Laatste dictaat');
    expect(uit).not.toContain('Mondig bewaart je dictaten niet');
  });

  it('toont bij 7 dagen de termijn bij de kop', () => {
    dictatenStand.termijn = '7d';
    dictatenStand.dictaten = [
      { id: 'dictaat-1-0.json', tijdUnixMs: Date.now() - 60_000, tekst: 'Een dictaat.' }
    ];
    expect(html()).toContain('7 dagen bewaard');
  });

  it('toont zonder bewaarde dictaten de lege staat', () => {
    const uit = html();
    expect(uit).toContain('Hier komt wat je dicteert');
  });

  it('wacht met de lijst tot Rust heeft geantwoord, dan geen lege staat die kan flitsen', () => {
    dictatenStand.geladen = false;
    const uit = html();
    expect(uit).not.toContain('Hier komt wat je dicteert');
    expect(uit).not.toContain('Je dictaten');
  });

  it('toont bij Niet bewaren een gewone kaart met een gedempte tekst en een tweede knop, zonder Klaar in of Geplakt', () => {
    dictatenStand.termijn = 'niet';
    dictaatStand.weergave = {
      type: 'klaar',
      taak: 1,
      uitslag: {
        taak: 1,
        uitkomst: 'tekst',
        tekst: 'Dit is het laatste dictaat.',
        duurMs: 1300,
        soevereiniteit: 'lokaal'
      }
    };
    dictaatStand.plak = 'geplakt';
    // De tekst voor de schermlezer (sr-only) telt niet: het gaat om wat zichtbaar is.
    const uit = html().replace(/<p class="sr-only">.*?<\/p>/, '');
    expect(uit).toContain('text-gedempt whitespace-pre-wrap');
    expect(uit).toContain('border-lijn bg-verhoogd');
    expect(uit).not.toContain('bg-actie font-semibold');
    expect(uit).not.toContain('Klaar in');
    expect(uit).not.toContain('Lokaal verwerkt');
    expect(uit).not.toContain('Geplakt waar je typte.');
  });

  it('houdt het proefveld op de pagina als de lijst leeg is, ook tijdens het dictaat', () => {
    const sneltoets = {
      actief: 'Ctrl+Spatie',
      isTerugval: false,
      gewenst: 'Ctrl+Spatie',
      geprobeerd: ['Ctrl+Spatie']
    };
    const kaal = (): string => render(DictaatPaneel, { props: { sneltoets, toonRust: true } }).body;
    expect(kaal()).toContain('id="proefveld"');
    dictaatStand.weergave = { type: 'luistert', taak: 1, sinds: Date.now() };
    expect(kaal()).toContain('id="proefveld"');
    dictaatStand.weergave = { type: 'verwerkt', taak: 1 };
    expect(kaal()).toContain('id="proefveld"');
  });

  it('toont het laatste dictaat los boven de lijst als het schrijven naar schijf mislukte', () => {
    dictatenStand.dictaten = [
      { id: 'dictaat-1-0.json', tijdUnixMs: Date.now() - 60_000, tekst: 'Een ouder dictaat.' }
    ];
    dictatenStand.laatste = { tekst: 'Dit is het laatste dictaat.', id: null };
    const uit = html();
    expect(uit).toContain('Laatste dictaat');
    expect(uit).toContain('Dit is het laatste dictaat.');
    expect(uit.indexOf('Laatste dictaat')).toBeLessThan(uit.indexOf('Je dictaten'));
    expect(uit).not.toContain('Mondig bewaart je dictaten niet');
  });

  it('toont het laatste dictaat niet los als het als nieuwste in de lijst staat', () => {
    dictatenStand.dictaten = [
      {
        id: 'dictaat-1-0.json',
        tijdUnixMs: Date.now() - 60_000,
        tekst: 'Dit is het laatste dictaat.'
      }
    ];
    dictatenStand.laatste = { tekst: 'Dit is het laatste dictaat.', id: 'dictaat-1-0.json' };
    expect(html()).not.toContain('Laatste dictaat');
  });

  it('laat een verlopen dictaat weg uit de lijst, ook als Rust het nog niet wiste', () => {
    dictatenStand.dictaten = [
      { id: 'dictaat-1-0.json', tijdUnixMs: Date.now() - 25 * 3_600_000, tekst: 'Veel te oud.' }
    ];
    const uit = html();
    expect(uit).not.toContain('Veel te oud.');
    expect(uit).toContain('Hier komt wat je dicteert');
  });
});
