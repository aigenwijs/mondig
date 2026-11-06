import { describe, expect, it } from 'vitest';
import {
  DICTATEN_TERMIJNEN,
  STANDAARD_DICTATEN_TERMIJN,
  datumKop,
  filterVerlopen,
  laatsteLosTonen,
  groepeerPerDag,
  termijnUit,
  vraagVoorTermijn,
  type BewaardDictaat
} from './dictaten';

// Een moment in de lokale tijd, zodat de tests niet van de tijdzone van de machine afhangen.
const om = (maand: number, dag: number, uur: number, minuut: number): number =>
  new Date(2026, maand - 1, dag, uur, minuut).getTime();

const dictaat = (id: string, tijdUnixMs: number, tekst = id): BewaardDictaat => ({
  id,
  tijdUnixMs,
  tekst
});

describe('de termijn', () => {
  it('kent de drie keuzes uit ADR 0021, met 24 uur als standaard', () => {
    expect(DICTATEN_TERMIJNEN).toEqual(['niet', '24u', '7d']);
    expect(STANDAARD_DICTATEN_TERMIJN).toBe('24u');
  });

  it('valt bij een onbekende waarde terug op 24 uur', () => {
    expect(termijnUit('7d')).toBe('7d');
    expect(termijnUit('niet')).toBe('niet');
    expect(termijnUit('30d')).toBe('24u');
    expect(termijnUit(undefined)).toBe('24u');
    expect(termijnUit(7)).toBe('24u');
  });
});

describe('groepeerPerDag', () => {
  const nu = om(11, 4, 15, 0);

  it('zet Vandaag en Gisteren op de kalenderdag, niet op 24 uur terug', () => {
    const groepen = groepeerPerDag(
      [
        dictaat('a', om(11, 4, 14, 2)),
        dictaat('b', om(11, 4, 0, 5)),
        dictaat('c', om(11, 3, 16, 30)),
        dictaat('d', om(11, 3, 0, 1)),
        dictaat('e', om(11, 2, 23, 55))
      ],
      nu
    );
    expect(groepen.map((g) => [g.soort, g.dictaten.map((d) => d.id)])).toEqual([
      ['vandaag', ['a', 'b']],
      ['gisteren', ['c', 'd']],
      ['datum', ['e']]
    ]);
  });

  it('houdt de volgorde (nieuwste eerst) en maakt geen groepen zonder dictaten', () => {
    expect(groepeerPerDag([], nu)).toEqual([]);
    const groepen = groepeerPerDag(
      [dictaat('a', om(11, 1, 9, 0)), dictaat('b', om(10, 30, 9, 0))],
      nu
    );
    expect(groepen.map((g) => g.soort)).toEqual(['datum', 'datum']);
    expect(groepen.map((g) => g.dictaten[0]?.id)).toEqual(['a', 'b']);
  });

  it('rekent Gisteren goed over een maandgrens', () => {
    const eersteVanDeMaand = om(12, 1, 10, 0);
    const [groep] = groepeerPerDag([dictaat('a', om(11, 30, 22, 0))], eersteVanDeMaand);
    expect(groep?.soort).toBe('gisteren');
  });

  it('noemt een oudere dag met de datum', () => {
    const kop = datumKop(om(10, 28, 12, 0), 'nl');
    expect(kop).toContain('28');
    expect(kop.toLowerCase()).toContain('oktober');
  });
});

describe('de bevestigvraag bij een andere termijn', () => {
  it('komt alleen bij Niet bewaren, en alleen als er bewaarde dictaten zijn', () => {
    expect(vraagVoorTermijn('niet', 12)).toBe(true);
    expect(vraagVoorTermijn('niet', 1)).toBe(true);
    expect(vraagVoorTermijn('niet', 0)).toBe(false);
    expect(vraagVoorTermijn('24u', 12)).toBe(false);
    expect(vraagVoorTermijn('7d', 12)).toBe(false);
  });
});

describe('filterVerlopen', () => {
  const nu = om(11, 4, 15, 0);
  const UUR = 3_600_000;

  it('laat weg wat op of voorbij de termijn is, met dezelfde grens als Rust', () => {
    const lijst = [dictaat('net-erna', nu - 24 * UUR), dictaat('net-ervoor', nu - 24 * UUR + 1)];
    expect(filterVerlopen(lijst, '24u', nu).map((d) => d.id)).toEqual(['net-ervoor']);
    expect(filterVerlopen(lijst, '7d', nu)).toHaveLength(2);
  });

  it('laat een tijd meer dan een uur in de toekomst weg', () => {
    const lijst = [dictaat('kan', nu + UUR), dictaat('te-ver', nu + UUR + 1)];
    expect(filterVerlopen(lijst, '24u', nu).map((d) => d.id)).toEqual(['kan']);
  });

  it('laat bij Niet bewaren alles weg', () => {
    expect(filterVerlopen([dictaat('a', nu)], 'niet', nu)).toEqual([]);
  });
});

describe('laatsteLosTonen', () => {
  const lijst = [dictaat('nieuwste', 2), dictaat('ouder', 1)];

  it('toont niets als er geen laatste dictaat is', () => {
    expect(laatsteLosTonen(null, lijst)).toBe(false);
  });

  it('toont het niet los als het als nieuwste in de lijst staat (op id)', () => {
    expect(laatsteLosTonen({ tekst: 'nieuwste', id: 'nieuwste' }, lijst)).toBe(false);
  });

  it('toont het los als het niet op schijf staat, ook met dezelfde tekst als de nieuwste', () => {
    expect(laatsteLosTonen({ tekst: 'nieuwste', id: null }, lijst)).toBe(true);
    expect(laatsteLosTonen({ tekst: 'x', id: null }, [])).toBe(true);
  });

  it('toont het los als het een ander bestand is dan het nieuwste in de lijst', () => {
    expect(laatsteLosTonen({ tekst: 'ouder', id: 'ouder' }, lijst)).toBe(true);
  });
});
