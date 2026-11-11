import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import { t, zetTaal } from '#lib';
import { toonInMapLabel } from '../toonInMap';
import ToonInMap from './ToonInMap.svelte';

// "Tonen in Verkenner" (ontwerpsysteem §5). De aanroep van Rust (`bestand_toon_opgeslagen`) en
// de menu-variant (die een geopend menu nodig heeft) zijn in de mock met de hand gecontroleerd.

describe('toonInMapLabel', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('kiest het label per platform', () => {
    expect(toonInMapLabel('windows', t())).toBe('Tonen in Verkenner');
    expect(toonInMapLabel('linux', t())).toBe('Tonen in Verkenner');
    expect(toonInMapLabel('mac', t())).toBe('Toon in Finder');
  });

  it('volgt de taal', () => {
    zetTaal('en');
    expect(toonInMapLabel('windows', t())).toBe('Show in Explorer');
    expect(toonInMapLabel('mac', t())).toBe('Show in Finder');
  });
});

describe('ToonInMap als icoonknop', () => {
  it('heeft aria-label en tooltip gelijk, met een map-icoon dat verborgen is voor de schermlezer', () => {
    const uit = render(ToonInMap, { props: { taak: 7 } }).body;
    expect(uit).toContain('aria-label="Tonen in Verkenner"');
    expect(uit).toContain('title="Tonen in Verkenner"');
    expect(uit).toContain('<svg');
    expect(uit).toContain('aria-hidden="true"');
    // Een icoonknop is minstens 32 px.
    expect(uit).toMatch(/\bsize-8\b/);
  });

  it('werkt ook met een eigen aanroep in plaats van een taak (een verslag in de lijst)', () => {
    const uit = render(ToonInMap, { props: { toon: () => Promise.resolve() } }).body;
    expect(uit).toContain('aria-label="Tonen in Verkenner"');
    expect(uit).toContain('title="Tonen in Verkenner"');
  });

  it('toont op de Mac Toon in Finder', () => {
    const uit = render(ToonInMap, { props: { taak: 7, platform: 'mac' } }).body;
    expect(uit).toContain('aria-label="Toon in Finder"');
    expect(uit).toContain('title="Toon in Finder"');
    expect(uit).not.toContain('Verkenner');
  });
});
