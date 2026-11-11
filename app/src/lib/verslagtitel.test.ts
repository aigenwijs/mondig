import { describe, expect, it } from 'vitest';
import { en } from './i18n/en';
import { nl } from './i18n/nl';
import { bewaarTotTekst, gegevensDelen, verslagTitel, wanneerTekst } from './verslagtitel';

// De teksten rond een bewaard verslag (N-C3, ADR 0022): titel, gegevensregel in de lijst en de
// zin "Mondig bewaart dit verslag tot ...".

const om = (dag: number, uur: number, minuut = 0): number =>
  new Date(2026, 10, dag, uur, minuut).getTime();
// Woensdag 4 november 2026, 15:00.
const nu = om(4, 15);
const locale = 'nl-NL';

describe('de titel van een verslag (keuze A)', () => {
  const basis = { soort: 'online' as const };

  it('zet klant en onderwerp met een middenpunt ertussen', () => {
    expect(verslagTitel({ ...basis, klant: 'Gemeente Vlaardingen', onderwerp: 'Intake' }, nl)).toBe(
      'Gemeente Vlaardingen · Intake'
    );
  });

  it('toont met alleen een klant of alleen een onderwerp dat ene stuk', () => {
    expect(verslagTitel({ ...basis, klant: 'Gemeente Vlaardingen', onderwerp: '' }, nl)).toBe(
      'Gemeente Vlaardingen'
    );
    expect(verslagTitel({ ...basis, klant: '', onderwerp: 'Overleg speelveld' }, nl)).toBe(
      'Overleg speelveld'
    );
  });

  it('is zonder klant en onderwerp het soortwoord, met de datum in de metaregel', () => {
    expect(verslagTitel({ ...basis, klant: '', onderwerp: null }, nl)).toBe('Vergadering');
    expect(verslagTitel({ ...basis, klant: '  ', onderwerp: '  ' }, nl)).toBe('Vergadering');
    expect(verslagTitel({ soort: 'alleen', klant: '', onderwerp: null }, nl)).toBe('Notitie');
    expect(verslagTitel({ ...basis, klant: '', onderwerp: null }, en)).toBe('Meeting');
  });

  it('is een onderwerp dat nooit is aangeraakt (null) het soortwoord, in de taal van de app', () => {
    expect(verslagTitel({ ...basis, klant: 'Gemeente Vlaardingen', onderwerp: null }, en)).toBe(
      'Gemeente Vlaardingen · Meeting'
    );
    expect(verslagTitel({ soort: 'alleen', klant: 'Gemeente', onderwerp: null }, nl)).toBe(
      'Gemeente · Notitie'
    );
    expect(verslagTitel({ soort: 'alleen', klant: 'Gemeente', onderwerp: null }, en)).toBe(
      'Gemeente · Note'
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

  it('zegt "alleen ik" bij een notitie en In de ruimte in de ruimte zonder sprekers', () => {
    expect(
      gegevensDelen(
        { ...basis, soort: 'alleen', duurMs: 4 * 60_000, sprekers: null },
        nu,
        nl,
        locale
      )
    ).toEqual(['Vandaag, 14:00', '4 min', 'alleen ik']);
    expect(gegevensDelen({ ...basis, soort: 'ruimte', sprekers: null }, nu, nl, locale)[2]).toBe(
      'In de ruimte'
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

describe('wanneer Mondig het verslag verwijdert', () => {
  it('noemt verder dan morgen de datum, ook binnen een week', () => {
    // Dinsdag 10 november.
    expect(bewaarTotTekst(om(10, 14), nu, nl, locale)).toBe('Wordt op 10 november verwijderd');
    expect(bewaarTotTekst(om(30, 14), nu, nl, locale)).toBe('Wordt op 30 november verwijderd');
  });

  it('noemt vandaag en morgen met de tijd', () => {
    expect(bewaarTotTekst(om(5, 9), nu, nl, locale)).toBe('Wordt morgen om 09:00 verwijderd');
    expect(bewaarTotTekst(om(4, 23), nu, nl, locale)).toBe('Wordt vandaag om 23:00 verwijderd');
  });

  it('zet geen tijd bij een moment dat al voorbij is', () => {
    expect(bewaarTotTekst(om(4, 9), nu, nl, locale)).toBe('Wordt vandaag verwijderd');
  });

  it('zegt het ook in het Engels', () => {
    expect(bewaarTotTekst(om(12, 14), nu, en, 'en-GB')).toBe('Will be deleted on 12 November');
    expect(bewaarTotTekst(om(4, 23), nu, en, 'en-GB')).toBe('Will be deleted today at 23:00');
  });

  it('zegt niets als de bewaartermijn onbekend is', () => {
    expect(bewaarTotTekst(null, nu, nl, locale)).toBeNull();
  });
});
