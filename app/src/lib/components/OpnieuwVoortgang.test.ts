import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import type { NaOpnieuw, OpnieuwStand } from '#lib/vergadering.js';
import OpnieuwMelding from './OpnieuwMelding.svelte';
import OpnieuwVoortgang from './OpnieuwVoortgang.svelte';

// De balk tijdens en de melding na het opnieuw uitschrijven (N-C4, ADR 0023).

function balk(stand: OpnieuwStand): string {
  return render(OpnieuwVoortgang, { props: { stand, stop: vi.fn() } }).body;
}

function melding(na: NaOpnieuw): string {
  return render(OpnieuwMelding, { props: { na, terug: vi.fn() } }).body;
}

describe('de voortgang van opnieuw uitschrijven', () => {
  it('toont de rol, het percentage, de resterende tijd en Stoppen', () => {
    const uit = balk({ rol: 'nauwkeurig', voortgang: 34, restMs: 6 * 60_000 });
    expect(uit).toContain(
      'Opnieuw uitschrijven met het nauwkeurige model… 34% · nog ongeveer 6 minuten.'
    );
    expect(uit).toContain('Verbeteren kan weer als het klaar is.');
    expect(uit).toContain('Stoppen');
    expect(uit).toContain('role="status"');
  });

  it('laat zonder percentage of tijd dat deel weg', () => {
    const uit = balk({ rol: null, voortgang: null, restMs: null });
    expect(uit).toContain('Opnieuw uitschrijven…');
    expect(uit).not.toContain('%');
    expect(uit).not.toContain('nog ongeveer');
  });
});

describe('de melding na opnieuw uitschrijven', () => {
  it('telt de veranderde regels en biedt de vorige versie aan', () => {
    const uit = melding({ rol: 'nauwkeurig', veranderd: 14, verdwenen: 0 });
    expect(uit).toContain(
      'Opnieuw uitgeschreven met het nauwkeurige model. 14 regels zijn veranderd.'
    );
    expect(uit).toContain('Vorige versie terugzetten');
    expect(uit).not.toContain('opnieuw-verdwenen');
  });

  it('noemt eigen markeringen die niet meer in de tekst staan', () => {
    const uit = melding({ rol: 'extra_nauwkeurig', veranderd: 1, verdwenen: 2 });
    expect(uit).toContain('met het extra nauwkeurige model. 1 regel is veranderd.');
    expect(uit).toContain('2 markeringen staan niet meer in de tekst; lees het verslag na.');
  });

  it('zegt het als er niets veranderde', () => {
    expect(melding({ rol: 'nauwkeurig', veranderd: 0, verdwenen: 0 })).toContain(
      'Er is geen regel veranderd.'
    );
  });
});
