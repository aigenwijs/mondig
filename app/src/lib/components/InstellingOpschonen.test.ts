import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { OpschonenInfo } from '#lib/opschonen.js';
import InstellingOpschonen from './InstellingOpschonen.svelte';

// "Dictaten opschonen" in Instellingen (ADR 0027). Servermodus: de HTML is wat oog en
// schermlezer krijgen; de effecten (laden uit Rust) draaien hier niet.

const basis: OpschonenInfo = {
  stand: 'uit',
  aanbieder: 'mistral',
  naam: 'Mistral',
  label: 'europees',
  model: 'mistral-small-latest',
  bevestigd: false
};

const html = (i: Partial<OpschonenInfo> = {}): string =>
  render(InstellingOpschonen, { props: { begin: { ...basis, ...i } } }).body;

describe('Dictaten opschonen in Instellingen', () => {
  it('heeft drie keuzes met Uit als standaard', () => {
    const tekst = html();
    expect(tekst).toContain('Dictaten opschonen');
    expect(tekst).toContain('Uit');
    expect(tekst).toContain('Met een eigen sneltoets');
    expect(tekst).toContain('Bij elk dictaat');
  });

  it('noemt bij Mistral de aanbieder en het label, en dat namen niet vervangen worden', () => {
    const tekst = html();
    expect(tekst).toContain('Je tekst gaat naar Mistral (Europees).');
    expect(tekst).toContain('niet vervangen');
    expect(tekst).not.toContain('Kies eerst een taalmodel');
  });

  it('zegt bij Ollama dat de tekst op de computer blijft en dat het een goede videokaart vraagt', () => {
    const tekst = html({ aanbieder: 'ollama', naam: 'Ollama', label: 'lokaal', model: 'qwen3:8b' });
    expect(tekst).toContain('Je tekst blijft op deze computer (Lokaal).');
    expect(tekst).toContain('Werkt alleen vlot met een goede videokaart.');
  });

  it('is zonder aanbieder niet te bedienen en verwijst naar Notulen', () => {
    const tekst = html({ aanbieder: null, naam: null, label: null, model: null });
    expect(tekst).toContain('Kies eerst een taalmodel onder Notulen.');
    expect(tekst).toContain('Naar Notulen');
    expect(tekst).toContain('#notulen');
    expect(tekst).toMatch(/<select[^>]*disabled/);
  });

  it('toont de tweede sneltoets alleen bij "Met een eigen sneltoets"', () => {
    expect(html({ stand: 'eigenSneltoets', bevestigd: true })).toContain(
      'Sneltoets voor opschonen'
    );
    expect(html({ stand: 'bijElkDictaat', bevestigd: true })).not.toContain(
      'Sneltoets voor opschonen'
    );
    expect(html()).not.toContain('Sneltoets voor opschonen');
  });

  it('stelt de vraag niet vooraf', () => {
    expect(html()).not.toContain('Waar gaat de tekst heen?');
  });
});
