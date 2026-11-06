import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import { OPSCHOON_STANDEN, modelNaam, vraagEerst, type OpschonenInfo } from './opschonen';

// Dictaten opschonen (ADR 0027): de pure hulpjes van de interface.

const info = (deel: Partial<OpschonenInfo> = {}): OpschonenInfo => ({
  stand: 'uit',
  aanbieder: 'mistral',
  naam: 'Mistral',
  label: 'europees',
  model: 'mistral-small-latest',
  bevestigd: false,
  ...deel
});

describe('opschonen', () => {
  it('begint met Uit en kent drie keuzes', () => {
    expect(OPSCHOON_STANDEN).toEqual(['uit', 'eigenSneltoets', 'bijElkDictaat']);
  });

  it('zet het label van Rust om in gewone taal', () => {
    expect(nl.opschonenLabel('europees')).toBe('Europees');
    expect(en.opschonenLabel('lokaal')).toBe('Local');
  });

  it('noemt een model in gewone taal', () => {
    expect(modelNaam('mistral-small-latest')).toBe('Mistral Small');
    expect(modelNaam('qwen3:8b')).toBe('qwen3:8b');
  });

  it('stelt de vraag alleen bij aanzetten, met een aanbieder, zolang die niet beantwoord is', () => {
    expect(vraagEerst(info(), 'eigenSneltoets')).toBe(true);
    expect(vraagEerst(info(), 'bijElkDictaat')).toBe(true);
    expect(vraagEerst(info(), 'uit')).toBe(false);
    expect(vraagEerst(info({ bevestigd: true }), 'bijElkDictaat')).toBe(false);
    expect(vraagEerst(info({ aanbieder: null }), 'bijElkDictaat')).toBe(false);
  });

  it('zegt in de vraag waar de tekst heen gaat en dat namen niet vervangen worden', () => {
    const tekst = nl.opschonenVraagTekst('Mistral', 'Europees');
    expect(tekst).toContain('Mistral (Europees)');
    expect(tekst).toContain('niet vervangen');
    expect(tekst).toContain('letterlijke tekst blijft op deze computer');
  });
});
