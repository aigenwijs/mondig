import { render } from 'svelte/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { zetTaal } from '#lib/i18n/index.svelte.js';
import PersoonRegel from './PersoonRegel.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const regel = {
  sleutel: 1,
  startMs: 0,
  eindMs: 1000,
  tekst: 'Mijn BSN is 111222333.',
  markeringen: [{ soort: 'bsn' as const, van: 11, tot: 20 }]
};
const regels = [regel];

function html(vervangen: boolean, lijst: (typeof regel)[] = regels): string {
  return render(PersoonRegel, {
    props: {
      regels: lijst,
      persoon: { vervangen, eigen: [] },
      zetVervangen: () => Promise.resolve(null),
      haalWeg: () => Promise.resolve(null),
      markeer: () => Promise.resolve(null),
      meldFout: () => undefined,
      meldStatus: () => undefined
    }
  }).body;
}

// Strip de tags en de commentaarmarkeringen van Svelte; wat overblijft is wat je leest.
const leesbaar = (uitvoer: string): string =>
  uitvoer
    .replace(/<!--.*?-->/g, '')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ');

describe('persoonsgegevens als één regel', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('toont de teller op de knop', () => {
    expect(leesbaar(html(false))).toContain('1 persoonsgegeven');
  });

  it('zegt bij uit dat er niets wordt vervangen', () => {
    const tekst = leesbaar(html(false));
    expect(tekst).toMatch(/Niet vervangen\s+bij kopiëren en opslaan/);
    expect(tekst).not.toContain('door [BSN] en zo');
  });

  it('zegt bij aan wat er voor in de plaats komt', () => {
    const tekst = leesbaar(html(true));
    expect(tekst).toMatch(/Vervangen\s+door \[BSN\] en zo, bij kopiëren en opslaan/);
    expect(tekst).not.toContain('Niet vervangen');
  });

  it('heeft een vraagteken met "Hoe werkt dit?" als naam', () => {
    expect(html(false)).toContain('aria-label="Hoe werkt dit?"');
  });

  it('zegt in het Engels hetzelfde per stand', () => {
    zetTaal('en');
    expect(leesbaar(html(false))).toMatch(/Not replaced\s+when copying and saving/);
    expect(leesbaar(html(true))).toMatch(
      /Replaced\s+by \[BSN\] and so on, when copying and saving/
    );
  });

  it('geeft de schakelaar een naam en een beschrijving', () => {
    const uit = html(true);
    expect(uit).toContain('role="switch"');
    expect(uit).toContain('aria-checked="true"');
    expect(uit).toContain('aria-labelledby="persoon-switch"');
    expect(uit).toContain('aria-describedby="persoon-stand"');
  });

  it('heeft bij nul gevonden gegevens geen schakelaar', () => {
    const geenGegevens = [{ ...regel, tekst: 'Niets bijzonders.', markeringen: [] }];
    const uit = html(false, geenGegevens);
    expect(uit).not.toContain('role="switch"');
    expect(leesbaar(uit)).not.toContain('Niet vervangen');
    expect(leesbaar(uit)).toContain('Geen persoonsgegevens');
  });

  it('houdt de schakelaar als het vervangen aanstaat, ook bij nul gegevens', () => {
    const geenGegevens = [{ ...regel, tekst: 'Niets bijzonders.', markeringen: [] }];
    expect(html(true, geenGegevens)).toContain('role="switch"');
  });

  it('geeft de teller een vaste id, het doel van de focus als de schakelaar verdwijnt', () => {
    expect(html(false)).toContain('id="persoon-teller"');
  });
});
