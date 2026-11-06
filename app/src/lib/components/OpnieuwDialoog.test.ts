import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import type { OpnieuwKeuzes } from '#lib/vergadering.js';
import OpnieuwDialoog from './OpnieuwDialoog.svelte';

// De dialoog "Opnieuw uitschrijven" (N-C4, ADR 0023): welk model het verslag maakte, per rol een
// keuze met de tijd of de downloadgrootte, wat er blijft, en Annuleren en Opnieuw uitschrijven.
// Servermodus: de HTML is wat het oog en de schermlezer krijgen.

const NAUWKEURIG = {
  modelId: 'canary-1b-v2-q8',
  rol: 'nauwkeurig',
  aanwezig: true,
  grootteBytes: 1_144_290_016,
  duurMs: 9 * 60_000
} as const;

const KEUZES: OpnieuwKeuzes = {
  rol: 'snel',
  keuzes: [
    {
      modelId: 'canary-1b-v2-q8',
      rol: 'nauwkeurig',
      aanwezig: true,
      grootteBytes: 1_144_290_016,
      duurMs: 9 * 60_000
    },
    {
      modelId: 'whisper-large-v3-turbo-q8',
      rol: 'extra_nauwkeurig',
      aanwezig: false,
      grootteBytes: 2_100_000_000,
      duurMs: null
    }
  ]
};

function html(
  props: Partial<{
    keuzes: OpnieuwKeuzes | null;
    verbeterd: number;
    gekozen: string | null;
    fout: string | null;
  }> = {}
): string {
  return render(OpnieuwDialoog, {
    props: {
      keuzes: KEUZES,
      verbeterd: 0,
      opnameMs: 52 * 60_000,
      gekozen: 'canary-1b-v2-q8',
      annuleer: vi.fn(),
      start: vi.fn(),
      ...props
    }
  }).body;
}

describe('de dialoog Opnieuw uitschrijven', () => {
  it('noemt het model van het verslag bij zijn rol en toont de keuzes per rol', () => {
    const uit = html();
    expect(uit).toContain('id="opnieuw-kop"');
    expect(uit).toContain('Dit verslag is gemaakt met het snelle model.');
    expect(uit).toContain('Nauwkeurig model');
    expect(uit).toContain('Extra nauwkeurig model');
    expect(uit).toContain('Minder fouten. Duurt ongeveer 9 minuten voor deze 52 minuten.');
    // Geen modelnamen op het scherm (N-B).
    expect(uit).not.toContain('Canary');
    expect(uit).not.toContain('Whisper');
    expect(uit).toContain('role="radiogroup"');
  });

  it('toont bij een ontbrekend model de downloadgrootte en een knop naar Modellen', () => {
    const uit = html({ gekozen: 'whisper-large-v3-turbo-q8' });
    expect(uit).toContain('Nog niet op deze computer. Eerst downloaden: 2,1 GB.');
    expect(uit).toContain('Naar Modellen');
    expect(uit).toMatch(/href="[^"]*\/modellen"/);
    expect(uit).not.toMatch(/Opnieuw uitschrijven(<!---->)?<\/button/);
    // Met het aanwezige model gekozen: wel starten, geen knop naar Modellen.
    const gewoon = html();
    expect(gewoon).toMatch(/Opnieuw uitschrijven(<!---->)?<\/button/);
    expect(gewoon).not.toContain('Naar Modellen');
  });

  it('zegt zonder meting eerlijk dat het langer duurt', () => {
    const uit = html({
      keuzes: { ...KEUZES, keuzes: [{ ...NAUWKEURIG, duurMs: null }] }
    });
    expect(uit).toContain('Duurt langer dan het snelle model.');
    expect(uit).not.toContain('Duurt ongeveer');
  });

  it('noemt de verbeterde regels alleen als dat er meer dan nul zijn', () => {
    expect(html({ verbeterd: 0 })).not.toContain('Je verbeterde');
    expect(html({ verbeterd: 0 })).not.toContain('opnieuw-verbeterd');
    expect(html({ verbeterd: 1 })).toContain(
      'Je verbeterde 1 regel. Die wordt opnieuw uitgeschreven'
    );
    expect(html({ verbeterd: 2 })).toContain(
      'Je verbeterde 2 regels. Die worden opnieuw uitgeschreven; daarna kun je terug naar deze versie.'
    );
    // De twee regels over wat er blijft, staan er altijd.
    const uit = html();
    expect(uit).toContain('Sprekers en de namen die je gaf, blijven.');
    expect(uit).toContain('Wat je zelf markeerde, blijft gemarkeerd.');
  });

  it('toont tijdens het laden een regel en Annuleren, en een fout als melding', () => {
    const laden = html({ keuzes: null });
    expect(laden).toContain('Mondig kijkt welke modellen er zijn');
    expect(laden).toContain('Annuleren');
    expect(laden).toContain('aria-disabled="true"');
    expect(html({ fout: 'Het lukte niet.' })).toContain('role="alert"');
  });

  it('gebruikt tokens en geen vaste kleuren', () => {
    const uit = html();
    expect(uit).toContain('data-[state=checked]:border-actie');
    expect(uit).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
  });
});

describe('de dialoog bij het nauwkeurigste model', () => {
  it('zegt dat er geen nauwkeuriger model is en heeft geen startknop', () => {
    const uit = html({ keuzes: { rol: 'extra_nauwkeurig', keuzes: [] }, gekozen: null });
    expect(uit).toContain('Er is geen nauwkeuriger model om dit verslag mee uit te schrijven.');
    expect(uit).not.toMatch(/Opnieuw uitschrijven(<!---->)?<\/button/);
    expect(uit).toContain('Annuleren');
  });
});
