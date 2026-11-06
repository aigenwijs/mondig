import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import VerslagTitel from './VerslagTitel.svelte';

// De titel van het verslag (N-C3): één regel met een potlood, of het invoerveld om de naam te
// wijzigen. De effecten van Svelte draaien hier niet (servermodus).

const stil = () => undefined;

function html(waarde: string | null): string {
  return render(VerslagTitel, {
    props: {
      titel: 'Overleg buurtbijeenkomst',
      waarde,
      start: stil,
      bewaar: stil,
      annuleer: stil
    }
  }).body;
}

describe('de titel van het verslag', () => {
  it('toont de titel op één regel als kop, met een potlood dat bij aanwijzen en focus verschijnt', () => {
    const uit = html(null);
    expect(uit).toMatch(/<h1[^>]*id="vergader-focus"[^>]*>\s*Overleg buurtbijeenkomst/);
    expect(uit).toContain('truncate');
    expect(uit).toContain('title="Overleg buurtbijeenkomst"');
    expect(uit).toContain('aria-label="Naam wijzigen, Overleg buurtbijeenkomst"');
    expect(uit).toContain('group-hover:opacity-100');
    expect(uit).toContain('group-focus-within:opacity-100');
    expect(uit).not.toContain('<input');
  });

  it('toont tijdens het bewerken een invoerveld met de naam erin en een grens van 100 tekens', () => {
    const uit = html('Nieuwe naam');
    expect(uit).toContain('<input');
    expect(uit).toContain('value="Nieuwe naam"');
    expect(uit).toContain('maxlength="100"');
    expect(uit).toContain('aria-label="Naam van het verslag"');
    expect(uit).not.toContain('<h1');
  });
});
