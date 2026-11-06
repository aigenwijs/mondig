import { describe, expect, it, vi } from 'vitest';
import type { Advies, LaatsteControle } from './computer';

// laad() mag een nieuwere bewaar() nooit overschrijven: komt het antwoord van laad() later
// binnen dan dat van bewaar(), dan blijft de bewaarde controle staan.

let lees: (waarde: LaatsteControle | null) => void = () => undefined;
const bewaard: LaatsteControle = {
  dicteren: 'parakeet-ultra-q8',
  bestanden: 'canary-1b-v2-q8',
  videokaart: 'laten',
  datumMs: 2
};

vi.mock('./computer', () => ({
  laatsteControle: () =>
    new Promise<LaatsteControle | null>((klaar) => {
      lees = klaar;
    }),
  laatsteControleBewaar: () => Promise.resolve(bewaard)
}));

describe('de stand van de laatste controle', () => {
  it('laat een laat antwoord van laad() een nieuwere bewaar() niet overschrijven', async () => {
    const { ControleStand } = await import('./controlestand.svelte');
    const stand = new ControleStand();
    const laden = stand.laad();
    await stand.bewaar({} as Advies);
    expect(stand.controle).toEqual(bewaard);
    lees({ ...bewaard, bestanden: 'parakeet-ultra-q8', datumMs: 1 });
    await laden;
    expect(stand.controle).toEqual(bewaard);
  });

  it('neemt het antwoord van laad() over als er niets nieuwers kwam', async () => {
    const { ControleStand } = await import('./controlestand.svelte');
    const stand = new ControleStand();
    const laden = stand.laad();
    lees(null);
    await laden;
    expect(stand.controle).toBeNull();
    expect(stand.geladen).toBe(true);
  });
});
