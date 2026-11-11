import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import type { ModelRegel } from '#lib/modellen.js';
import { t } from '#lib';
import ModelBeheerKaart from './ModelBeheerKaart.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// Verwijderen bij een model in gebruik (UI-check punt 28) en de tekst "in gebruik voor ..."
// (punt 27).

function regel(over: Partial<ModelRegel>): ModelRegel {
  return {
    id: 'parakeet-ultra',
    naam: 'Parakeet Ultra',
    soort: 'spraak',
    rol: 'snel',
    nauwkeurigheid: 3,
    snelheid: 4,
    talen: ['nl', 'en'],
    status: 'compleet',
    grootteBytes: 700_000_000,
    aanwezigBytes: 700_000_000,
    tekortBytes: 0,
    nietVoor: [],
    inGebruikVoor: ['dicteren', 'bestanden'],
    kanWeg: false,
    nieuw: false,
    bron: '',
    daarnaBijHervatten: [],
    ...over
  };
}

const kaart = (r: ModelRegel): string =>
  render(ModelBeheerKaart, { props: { regel: r, nieuw: false } }).body;

describe('Verwijderen bij een model in gebruik', () => {
  it('is duidelijk uit: slotje, aanwijzer, reden als tooltip én zichtbaar eronder', () => {
    const uit = kaart(regel({}));
    expect(uit).toContain('aria-disabled="true"');
    expect(uit).toContain('aria-disabled:cursor-not-allowed');
    expect(uit).toContain('lucide-lock');
    expect(uit).toContain(`title="${t().nietWeg}"`);
    // Zichtbaar, niet alleen voor de schermlezer.
    expect(uit).toMatch(/<span id="niet-weg-parakeet-ultra" class="basis-full text-dim"/);
  });

  it('is gewoon aan bij een model dat niet in gebruik is', () => {
    const uit = kaart(regel({ inGebruikVoor: [], kanWeg: true }));
    expect(uit).not.toContain('lucide-lock');
    expect(uit).not.toContain('niet-weg-');
    expect(uit).toContain('lucide-trash-2');
  });
});

describe('in gebruik voor ...', () => {
  it('noemt opnames en bestanden', () => {
    expect(t().inGebruikVoorBestanden).toBe('in gebruik voor opnames en bestanden');
    expect(t().inGebruikVoorBeide).toBe('in gebruik voor dicteren, opnames en bestanden');
    expect(kaart(regel({ inGebruikVoor: ['bestanden'] }))).toContain(
      'in gebruik voor opnames en bestanden'
    );
  });
});
