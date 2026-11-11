import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { en } from '#lib/i18n/en.js';
import { nl } from '#lib/i18n/nl.js';
import InstellingStopwoorden from './InstellingStopwoorden.svelte';

describe('Opvulklanken weghalen', () => {
  it('is een rij met schakelaar, kop en uitleg in gewone taal', () => {
    const html = render(InstellingStopwoorden).body;
    expect(html).toContain('role="switch"');
    expect(html).toContain('Opvulklanken weghalen');
    expect(html).toContain('uitschriften van bestanden en opnames');
  });

  it('heeft een Engelse vertaling die niet gelijk is aan de Nederlandse', () => {
    expect(en.stopwoordenKop).not.toBe(nl.stopwoordenKop);
    expect(en.stopwoordenHulp).not.toBe(nl.stopwoordenHulp);
  });

  it('staat onder Dicteren, achter Meer opties', () => {
    const bron = readFileSync(join(__dirname, '../../routes/instellingen/+page.svelte'), 'utf-8');
    const dicteren = bron.indexOf('id="sectie-dicteren"');
    const opslaan = bron.indexOf('id="sectie-opslaan"');
    const rij = bron.indexOf('<InstellingStopwoorden />');
    expect(rij).toBeGreaterThan(dicteren);
    expect(rij).toBeLessThan(opslaan);
    expect(bron.slice(dicteren, rij)).toContain('<MeerOpties>');
  });
});
