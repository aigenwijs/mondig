import { describe, expect, it } from 'vitest';
import { alsWijzigFout, maakVolgorde } from './sneltoets';

describe('maakVolgorde', () => {
  it('houdt het laatste antwoord voor het huidige', () => {
    const volgorde = maakVolgorde();
    const eerste = volgorde.nieuw();
    expect(eerste()).toBe(true);
    const tweede = volgorde.nieuw();
    // Het trage antwoord van de eerste aanvraag komt na de tweede: negeren.
    expect(eerste()).toBe(false);
    expect(tweede()).toBe(true);
  });

  it('maakt een lopend antwoord ongeldig zonder een nieuwe aanvraag', () => {
    const volgorde = maakVolgorde();
    const antwoord = volgorde.nieuw();
    volgorde.ongeldig();
    expect(antwoord()).toBe(false);
  });
});

describe('alsWijzigFout', () => {
  it('herkent een weigering en een bezette toets', () => {
    expect(alsWijzigFout({ soort: 'geweigerd', fout: 'werktNiet' })).toEqual({
      soort: 'geweigerd',
      fout: 'werktNiet'
    });
    expect(alsWijzigFout({ soort: 'bezet' })).toEqual({ soort: 'bezet' });
    // De andere sneltoets van Mondig (ADR 0027).
    expect(alsWijzigFout({ soort: 'dubbel' })).toEqual({ soort: 'dubbel' });
  });

  it('geeft null bij elke andere fout', () => {
    expect(alsWijzigFout('kapot')).toBeNull();
    expect(alsWijzigFout(null)).toBeNull();
    expect(alsWijzigFout({ melding: 'x', vervolg: 'geen' })).toBeNull();
    expect(alsWijzigFout({ soort: 'geweigerd' })).toBeNull();
  });
});
