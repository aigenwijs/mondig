import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { MISTRAL_NAUWKEURIG, type NotulenInstellingen } from '#lib/notulen.js';
import InstellingNotulen from './InstellingNotulen.svelte';

// De groep Notulen in Instellingen (T3). Servermodus: de HTML is wat oog en schermlezer krijgen.

const basis: NotulenInstellingen = {
  aanbieder: 'mistral',
  mistralModel: MISTRAL_NAUWKEURIG,
  ollamaModel: null,
  trainingUit: true,
  sleutelStaat: true,
  sleutelFout: null
};

const html = (i: Partial<NotulenInstellingen> = {}): string =>
  render(InstellingNotulen, { props: { begin: { ...basis, ...i } } }).body;

describe('Notulen in Instellingen', () => {
  it('is zonder keuze alleen de rij "Notulen maken met", met Niet ingesteld', () => {
    const tekst = html({ aanbieder: null });
    expect(tekst).toContain('Notulen maken met');
    expect(tekst).toContain('Niet ingesteld');
    expect(tekst).toContain('Mistral (Europees)');
    expect(tekst).toContain('Ollama op deze computer');
    expect(tekst).toContain('Niets ingesteld: dan maakt Mondig geen notulen.');
    expect(tekst).not.toContain('Sleutel van Mistral');
    expect(tekst).not.toContain('Training staat uit');
  });

  it('toont bij Mistral met een sleutel alleen "Opgeslagen", Testen en Verwijderen, geen sleutel', () => {
    const tekst = html();
    expect(tekst).toContain('Opgeslagen');
    expect(tekst).toContain('Testen');
    expect(tekst).toContain('Verwijderen');
    expect(tekst).not.toContain('type="password"');
  });

  it('toont zonder sleutel een gemaskeerd veld dat leeg begint, en Bewaren', () => {
    const tekst = html({ sleutelStaat: false });
    expect(tekst).toContain('type="password"');
    expect(tekst).toContain('Bewaren');
    expect(tekst).toMatch(/<input[^>]*type="password"[^>]*value=""/);
    expect(tekst).not.toContain('Testen');
  });

  it('heeft bij Mistral het vinkje over training, met de uitleg achter een knop', () => {
    const tekst = html();
    expect(tekst).toContain('Training staat uit voor deze sleutel.');
    expect(tekst).toContain('Waarom vraagt Mondig dit?');
    expect(tekst).toContain('aria-expanded="false"');
    // De uitleg staat pas na een klik in beeld.
    expect(tekst).not.toContain('verwerkersovereenkomst');
  });

  it('toont het vinkje als aan zodra Rust zegt dat training uit staat, en anders niet', () => {
    expect(html({ trainingUit: true })).toMatch(/aria-checked="true"|data-state="checked"/);
    expect(html({ trainingUit: false })).toMatch(/aria-checked="false"|data-state="unchecked"/);
  });

  it('biedt de twee modellen van Mistral, Nauwkeurig als standaard', () => {
    const tekst = html();
    expect(tekst).toContain('Nauwkeurig (Mistral Large)');
    expect(tekst).toContain('Sneller (Mistral Small)');
    expect(tekst).toContain('value="mistral-large-latest"');
  });

  it('toont bij Ollama de waarschuwing over de duur, zonder sleutelveld of vinkje', () => {
    const tekst = html({ aanbieder: 'ollama', trainingUit: false, sleutelStaat: false });
    expect(tekst).toContain('Model van Ollama');
    expect(tekst).toContain('Zonder snelle videokaart duurt dit 10 tot 20 minuten.');
    expect(tekst).not.toContain('type="password"');
    expect(tekst).not.toContain('Training staat uit');
  });

  it('meldt een probleem met de sleutelbos in gewone taal', () => {
    const tekst = html({
      sleutelStaat: false,
      sleutelFout: 'De sleutelbos van deze computer gaf een fout.'
    });
    expect(tekst).toContain('De sleutelbos van deze computer gaf een fout.');
  });
});
