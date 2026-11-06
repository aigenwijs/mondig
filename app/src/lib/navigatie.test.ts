import { describe, expect, it } from 'vitest';
import { bekendeRoute } from './navigatie';

describe('navigeren vanuit de app', () => {
  it('kent de routes van het venster', () => {
    expect(bekendeRoute('/modellen')).toBe('/modellen');
    expect(bekendeRoute('/instellingen')).toBe('/instellingen');
    expect(bekendeRoute('/over')).toBe('/over');
    expect(bekendeRoute('/bestanden')).toBe('/bestanden');
    expect(bekendeRoute('/vergadering')).toBe('/vergadering');
    expect(bekendeRoute('/')).toBe('/');
  });

  it('weigert alles wat geen bekende route is', () => {
    expect(bekendeRoute('/pil')).toBeNull();
    // De proefpagina is weg (UX-ronde na N-C).
    expect(bekendeRoute('/dicteren')).toBeNull();
    expect(bekendeRoute('https://example.com')).toBeNull();
    expect(bekendeRoute('/modellen/../x')).toBeNull();
  });
});
