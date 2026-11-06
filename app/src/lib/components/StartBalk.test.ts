import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createRawSnippet } from 'svelte';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import StartBalk from './StartBalk.svelte';

// "Opname starten" staat direct onder de apparaten, vóór de lijsten (gebruikerstest na N-C), als
// gewone balk: niet zwevend, zodat hij niet over de lijst heen ligt.
const knop = createRawSnippet(() => ({ render: () => '<button>Opname starten</button>' }));

function balk(): string {
  return render(StartBalk, { props: { children: knop } }).body;
}

describe('de startbalk', () => {
  it('is een gewone balk, niet zwevend', () => {
    const html = balk();
    expect(html).toContain('Opname starten');
    expect(html).not.toMatch(/\bsticky\b/);
    expect(html).not.toMatch(/\bfixed\b/);
  });

  it('staat op Opnemen na de apparaten en vóór Nog uit te schrijven en Eerder opgenomen', () => {
    const bron = readFileSync(join(__dirname, '../../routes/vergadering/+page.svelte'), 'utf-8');
    const apparaten = bron.indexOf('id="vergader-mic-kop"');
    const balkPlek = bron.indexOf('<StartBalk>');
    const wachtend = bron.indexOf('{#if wachtend.length > 0}');
    const eerder = bron.indexOf('<EerderOpgenomen');
    expect(apparaten).toBeGreaterThan(0);
    expect(balkPlek).toBeGreaterThan(apparaten);
    expect(wachtend).toBeGreaterThan(balkPlek);
    expect(eerder).toBeGreaterThan(balkPlek);
    // Tussen de apparaten en de balk is elke {#if} weer gesloten: de balk hangt niet aan een
    // voorwaarde van de apparaatkaart, maar staat er altijd.
    const ertussen = bron.slice(apparaten, balkPlek);
    const geopend = (ertussen.match(/\{#if /g) ?? []).length;
    const gesloten = (ertussen.match(/\{\/if\}/g) ?? []).length;
    expect(geopend).toBe(gesloten);
  });
});
