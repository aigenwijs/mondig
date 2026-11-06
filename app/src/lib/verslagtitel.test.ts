import { describe, expect, it } from 'vitest';
import { nl } from './i18n/nl';
import {
  bewaarTotTekst,
  gegevensDelen,
  standaardTitel,
  verslagTitel,
  wanneerTekst
} from './verslagtitel';

// De teksten rond een bewaard verslag (N-C3, ADR 0022): titel, gegevensregel in de lijst en de
// zin "Mondig bewaart dit verslag tot ...".

const om = (dag: number, uur: number, minuut = 0): number =>
  new Date(2026, 10, dag, uur, minuut).getTime();
// Woensdag 4 november 2026, 15:00.
const nu = om(4, 15);
const locale = 'nl-NL';

describe('de titel van een verslag', () => {
  it('maakt zelf een titel uit soort en begin', () => {
    expect(standaardTitel('online', om(2, 14), nl, locale)).toBe(
      'Vergadering van 2 november, 14:00'
    );
    expect(standaardTitel('ruimte', om(2, 14), nl, locale)).toBe(
      'Vergadering van 2 november, 14:00'
    );
    expect(standaardTitel('alleen', om(2, 9, 5), nl, locale)).toBe('Notitie van 2 november, 09:05');
  });

  it('toont de naam die de gebruiker gaf, anders de titel van Mondig', () => {
    const basis = { soort: 'online' as const, beginMs: om(2, 14) };
    expect(verslagTitel({ ...basis, naam: 'Overleg speelveld' }, nl, locale)).toBe(
      'Overleg speelveld'
    );
    expect(verslagTitel({ ...basis, naam: null }, nl, locale)).toBe(
      'Vergadering van 2 november, 14:00'
    );
    expect(verslagTitel({ ...basis, naam: '   ' }, nl, locale)).toBe(
      'Vergadering van 2 november, 14:00'
    );
  });
});

describe('de gegevens onder de titel in de lijst', () => {
  const basis = { soort: 'online' as const, beginMs: om(4, 14), duurMs: 52 * 60_000 };

  it('noemt moment, duur en het aantal sprekers', () => {
    expect(gegevensDelen({ ...basis, sprekers: 3 }, nu, nl, locale)).toEqual([
      'Vandaag, 14:00',
      '52 min',
      '3 sprekers'
    ]);
    expect(gegevensDelen({ ...basis, sprekers: 1 }, nu, nl, locale)[2]).toBe('1 spreker');
  });

  it('noemt gisteren en oudere dagen bij de naam van de dag of de datum', () => {
    expect(
      gegevensDelen({ ...basis, beginMs: om(3, 17, 12), sprekers: null }, nu, nl, locale)[0]
    ).toBe('Gisteren, 17:12');
    expect(
      gegevensDelen(
        { ...basis, beginMs: new Date(2026, 9, 30, 10).getTime(), sprekers: null },
        nu,
        nl,
        locale
      )[0]
    ).toBe('30 oktober, 10:00');
    expect(wanneerTekst(om(4, 9, 15), nu, nl, locale)).toBe('vandaag, 09:15');
  });

  it('zegt "alleen ik" bij een notitie en Fysieke vergadering in de ruimte zonder sprekers', () => {
    expect(
      gegevensDelen(
        { ...basis, soort: 'alleen', duurMs: 4 * 60_000, sprekers: null },
        nu,
        nl,
        locale
      )
    ).toEqual(['Vandaag, 14:00', '4 min', 'alleen ik']);
    expect(gegevensDelen({ ...basis, soort: 'ruimte', sprekers: null }, nu, nl, locale)[2]).toBe(
      'Fysieke vergadering'
    );
  });

  it('noemt een duur vanaf een uur als "1 uur 4 min", zoals in het ontwerp', () => {
    const delen = (minuten: number): string | undefined =>
      gegevensDelen({ ...basis, duurMs: minuten * 60_000, sprekers: null }, nu, nl, locale)[1];
    expect(delen(52)).toBe('52 min');
    expect(delen(60)).toBe('1 uur');
    expect(delen(64)).toBe('1 uur 4 min');
    expect(delen(125)).toBe('2 uur 5 min');
    expect(nl.vergaderMinutenKort(1)).toBe('1 min');
  });

  it('heeft online zonder herkende sprekers geen derde deel', () => {
    expect(gegevensDelen({ ...basis, sprekers: null }, nu, nl, locale)).toHaveLength(2);
  });
});

describe('Mondig bewaart dit verslag tot ...', () => {
  it('noemt een weekdag als dat binnen een week is', () => {
    // Dinsdag 10 november.
    expect(bewaarTotTekst(om(10, 14), nu, nl, locale)).toBe(
      'Mondig bewaart dit verslag tot dinsdag'
    );
  });

  it('noemt morgen en vandaag', () => {
    expect(bewaarTotTekst(om(5, 9), nu, nl, locale)).toBe('Mondig bewaart dit verslag tot morgen');
    expect(bewaarTotTekst(om(4, 23), nu, nl, locale)).toBe(
      'Mondig bewaart dit verslag tot vandaag'
    );
  });

  it('noemt verder weg een datum', () => {
    expect(bewaarTotTekst(om(30, 14), nu, nl, locale)).toBe(
      'Mondig bewaart dit verslag tot 30 november'
    );
  });

  it('zegt niets als de bewaartermijn onbekend is', () => {
    expect(bewaarTotTekst(null, nu, nl, locale)).toBeNull();
  });
});
