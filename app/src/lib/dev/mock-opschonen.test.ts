import { describe, expect, it } from 'vitest';
import type { NotulenInstellingen } from '../notulen';
import { maakOpschonenMock } from './mock-opschonen';

// De dev-mock van opschonen volgt Rust: aanzetten alleen met een aanbieder, en de eerste keer per
// aanbieder alleen met een bevestiging.

const mistral: NotulenInstellingen = {
  aanbieder: 'mistral',
  mistralModel: 'mistral-large-latest',
  ollamaModel: null,
  trainingUit: true,
  sleutelStaat: true,
  sleutelFout: null
};

describe('de mock van opschonen', () => {
  it('weigert aanzetten zonder aanbieder', () => {
    const mock = maakOpschonenMock(new Set(), () => null);
    expect(() => mock.handel('opschonen_zet_stand', { stand: 'bijElkDictaat' })).toThrow();
    expect(mock.stand()).toBe('uit');
  });

  it('vraagt de eerste keer een bevestiging voor de aanbieder', () => {
    const mock = maakOpschonenMock(new Set(), () => mistral);
    expect(() => mock.handel('opschonen_zet_stand', { stand: 'eigenSneltoets' })).toThrow();
    mock.handel('opschonen_zet_stand', { stand: 'eigenSneltoets', bevestigd: 'mistral' });
    expect(mock.stand()).toBe('eigenSneltoets');
    // Met de gewone toets letterlijk, met de tweede opgeschoond, bij Mistral met Mistral Small.
    expect(mock.plan(false)).toBeNull();
    expect(mock.plan(true)).toEqual({
      label: 'europees',
      model: 'mistral-small-latest',
      aanbieder: 'mistral'
    });
  });

  it('schoont bij elk dictaat op, ook met de gewone toets', () => {
    const mock = maakOpschonenMock(new Set(['opschonenelk']), () => mistral);
    expect(mock.plan(false)?.label).toBe('europees');
  });
});
