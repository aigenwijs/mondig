import { render } from 'svelte/server';
import { afterEach, describe, expect, it } from 'vitest';
import { t, zetTaal } from '#lib';
import BewaartermijnRegel from './BewaartermijnRegel.svelte';

// De bewaartermijnzin (UI-check punt 9, 10 en 13): één formulering, en de termijn komt steeds
// uit de instelling, nooit vast uit de tekst.

describe('de zin over de bewaartermijn van opnames', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('noemt de termijn uit de instelling en linkt naar Wijzigen', () => {
    for (const [dagen, tekst] of [
      [1, '1 dag'],
      [7, '7 dagen'],
      [30, '30 dagen']
    ] as const) {
      const uit = render(BewaartermijnRegel, { props: { dagen } }).body;
      expect(uit).toContain(`Mondig verwijdert opnames na ${tekst}.`);
      expect(uit).toMatch(/href="[^"]*\/instellingen#bewaartermijn"[^>]*>(<!---->)?Wijzigen/);
    }
  });

  it('volgt de taal', () => {
    zetTaal('en');
    const uit = render(BewaartermijnRegel, { props: { dagen: 14 } }).body;
    expect(uit).toContain('Mondig deletes recordings after 14 days.');
  });

  it('zegt bij Later uitschrijven dezelfde termijn en schrijft niets vast', () => {
    expect(t().vergaderLaterHulp(7)).toBe(
      'De opname blijft 7 dagen bewaard. Schrijf hem binnen die tijd uit.'
    );
    expect(t().vergaderLaterHulp(30)).toContain('30 dagen');
    expect(t().vergaderLaterHulp(1)).toContain('1 dag bewaard');
    expect(t().vergaderLaterHulp(null)).not.toMatch(/\d/);
  });
});
