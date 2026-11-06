import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { BewaardDictaat } from '#lib/dictaten.js';
import DictatenLijst from './DictatenLijst.svelte';

// "Je dictaten" (ADR 0021): de lijst per dag, de lege staat en de bediening. De effecten van
// Svelte draaien hier niet (servermodus); de HTML is wat een schermlezer en het oog krijgen.

const om = (dag: number, uur: number, minuut: number): number =>
  new Date(2026, 10, dag, uur, minuut).getTime();
const nu = om(4, 15, 0);

const dictaten: BewaardDictaat[] = [
  { id: 'dictaat-1-0.json', tijdUnixMs: om(4, 14, 2), tekst: 'Ik stuur de stukken morgen rond.' },
  {
    id: 'dictaat-2-0.json',
    tijdUnixMs: om(4, 11, 47),
    tekst: 'Beste Sanne, dank voor je reactie.'
  },
  {
    id: 'dictaat-3-0.json',
    tijdUnixMs: om(3, 16, 30),
    tekst: 'Notitie voor het verslag van de raad.'
  }
];

const stil = () => Promise.resolve();

function html(
  props: Partial<{ dictaten: BewaardDictaat[]; termijn: '24u' | '7d'; beginOpen: string }> = {}
): string {
  return render(DictatenLijst, {
    props: {
      dictaten,
      termijn: '24u' as const,
      bewaard: dictaten.length,
      nu,
      bijVerwijder: stil,
      bijWisAlles: stil,
      bijHoeLang: () => undefined,
      ...props
    }
  }).body;
}

describe('de lijst Je dictaten', () => {
  it('toont de kop en de termijn', () => {
    const uit = html();
    expect(uit).toContain('Je dictaten');
    expect(uit).toContain('24 uur bewaard');
    expect(html({ termijn: '7d' })).toContain('7 dagen bewaard');
  });

  it('groepeert per dag: Vandaag en Gisteren, met de tijd en de tekst per regel', () => {
    const uit = html();
    expect(uit).toContain('Vandaag');
    expect(uit).toContain('Gisteren');
    expect(uit.indexOf('Vandaag')).toBeLessThan(uit.indexOf('Gisteren'));
    for (const tijd of ['14:02', '11:47', '16:30']) expect(uit).toContain(tijd);
    expect(uit).toContain('Ik stuur de stukken morgen rond.');
    expect(uit).toContain('Notitie voor het verslag van de raad.');
    // De tekst van vandaag staat voor die van gisteren.
    expect(uit.indexOf('Ik stuur de stukken')).toBeLessThan(
      uit.indexOf('Notitie voor het verslag')
    );
  });

  it('kapt de tekst op een regel af en klapt open met een knop met aria-expanded', () => {
    const uit = html();
    expect(uit).toContain('aria-expanded="false"');
    expect(uit).toMatch(/class="[^"]*\btruncate\b[^"]*"[^>]*>Ik stuur de stukken/);
  });

  it('geeft elke regel een Kopiëren- en een Verwijderen-knop met de tijd in de naam', () => {
    const uit = html();
    expect(uit).toContain('aria-label="Dictaat van 14:02 kopiëren"');
    expect(uit).toContain('aria-label="Dictaat van 14:02 verwijderen"');
    expect((uit.match(/aria-label="Dictaat van [^"]* verwijderen"/g) ?? []).length).toBe(3);
  });

  it('toont de acties bij aanwijzen en bij toetsenbordfocus, niet alleen bij de muis', () => {
    const uit = html();
    expect(uit).toContain('group-hover:opacity-100');
    expect(uit).toContain('group-focus-within:opacity-100');
  });

  it('heeft de actieknoppen van minstens 32 px (size-8)', () => {
    const uit = html();
    const knop = /<button[^>]*aria-label="Dictaat van 14:02 verwijderen"[^>]*>/.exec(uit)?.[0];
    expect(knop).toMatch(/\bsize-8\b/);
  });

  it('heeft een menu met de kop als naam, bereikbaar voor het toetsenbord', () => {
    const uit = html();
    expect(uit).toContain('id="dictaten-menu"');
    expect(uit).toContain('aria-label="Meer opties voor je dictaten"');
  });

  it('toont zonder dictaten de lege staat met de zin over de termijn', () => {
    const uit = html({ dictaten: [] });
    expect(uit).toContain('Hier komt wat je dicteert');
    expect(uit).toContain('blijft hier 24 uur staan');
    expect(uit).not.toContain('Vandaag');
    expect(html({ dictaten: [], termijn: '7d' })).toContain('blijft hier 7 dagen staan');
  });

  it('toont de bevestigvraag niet vanzelf', () => {
    expect(html()).not.toContain('role="alertdialog"');
  });
});

describe('een opgeschoond dictaat (ADR 0027)', () => {
  const opgeschoond: BewaardDictaat = {
    id: 'dictaat-9-0.json',
    tijdUnixMs: om(4, 14, 30),
    tekst: 'We zien elkaar woensdag.',
    letterlijk: 'ehm we zien elkaar dinsdag nee woensdag',
    opgeschoondDoor: { model: 'mistral-small-latest', aanbieder: 'mistral', label: 'europees' }
  };

  it('toont opengeklapt wie het opschoonde en de knop voor de letterlijke tekst', () => {
    const uit = html({ dictaten: [opgeschoond], beginOpen: opgeschoond.id });
    expect(uit).toContain('Opgeschoond door Mistral Small (Europees)');
    expect(uit).toContain('Letterlijke tekst kopiëren');
    // De letterlijke tekst zelf staat niet in beeld, alleen achter de knop.
    expect(uit).not.toContain('dinsdag nee woensdag');
  });

  it('toont dat dichtgeklapt niet, en nooit bij een dictaat zonder opschonen', () => {
    expect(html({ dictaten: [opgeschoond] })).not.toContain('Opgeschoond door');
    expect(html({ beginOpen: 'dictaat-1-0.json' })).not.toContain('Opgeschoond door');
  });

  it('noemt bij Ollama het model en Lokaal', () => {
    const lokaal: BewaardDictaat = {
      ...opgeschoond,
      opgeschoondDoor: { model: 'qwen3:8b', aanbieder: 'ollama', label: 'lokaal' }
    };
    expect(html({ dictaten: [lokaal], beginOpen: lokaal.id })).toContain(
      'Opgeschoond door qwen3:8b (Lokaal)'
    );
  });

  it('raadt geen label als Rust er geen gaf (een ouder bestand)', () => {
    const oud: BewaardDictaat = {
      ...opgeschoond,
      opgeschoondDoor: { model: 'mistral-small-latest', aanbieder: 'mistral' }
    };
    const uit = html({ dictaten: [oud], beginOpen: oud.id });
    expect(uit).toContain('Opgeschoond door Mistral Small');
    expect(uit).not.toContain('(Europees)');
  });
});
