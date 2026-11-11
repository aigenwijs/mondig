import { describe, expect, it } from 'vitest';
import {
  DICTATEN_TERMIJNEN,
  STANDAARD_DICTATEN_TERMIJN,
  datumKop,
  filterVerlopen,
  laatsteLosTonen,
  groepeerPerDag,
  termijnUit,
  herbevestig,
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
  it('kent de vijf keuzes uit ADR 0021 en de aanvulling, met 24 uur als standaard', () => {
    expect(DICTATEN_TERMIJNEN).toEqual(['niet', 'laatste10', '24u', '7d', '30d']);
    expect(STANDAARD_DICTATEN_TERMIJN).toBe('24u');
  });

  it('valt bij een onbekende waarde terug op 24 uur', () => {
    expect(termijnUit('7d')).toBe('7d');
    expect(termijnUit('niet')).toBe('niet');
    expect(termijnUit('30d')).toBe('30d');
    expect(termijnUit('laatste10')).toBe('laatste10');
    expect(termijnUit('30 dagen')).toBe('24u');
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

describe('groepeerPerDag over de delen heen', () => {
  it('houdt een dag bij elkaar als hij over de grens van het eerste deel loopt', () => {
    const nu = om(11, 4, 15, 0);
    const alle = Array.from({ length: 14 }, (_, i) => dictaat(`d${String(i)}`, nu - i * 60_000));
    // Het eerste deel van tien en daarna alle veertien: dezelfde ene groep Vandaag.
    expect(groepeerPerDag(alle.slice(0, 10), nu)).toHaveLength(1);
    const groepen = groepeerPerDag(alle, nu);
    expect(groepen).toHaveLength(1);
    expect(groepen[0]?.soort).toBe('vandaag');
    expect(groepen[0]?.dictaten).toHaveLength(14);
  });
});

describe('herbevestig', () => {
  it('gaat door als het getal gelijk is of er niets meer wegvalt', async () => {
    expect(await herbevestig(5, () => Promise.resolve(5))).toEqual({ doorgaan: true, aantal: 5 });
    expect(await herbevestig(5, () => Promise.resolve(0))).toEqual({ doorgaan: true, aantal: 0 });
  });

  it('blijft vragen met het nieuwe getal als het veranderd is', async () => {
    expect(await herbevestig(5, () => Promise.resolve(6))).toEqual({ doorgaan: false, aantal: 6 });
    expect(await herbevestig(5, () => Promise.resolve(4))).toEqual({ doorgaan: false, aantal: 4 });
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

  it('kent 30 dagen en laat bij Laatste 10 niets op leeftijd weg', () => {
    const DAG = 24 * UUR;
    const lijst = [dictaat('net-erna', nu - 30 * DAG), dictaat('net-ervoor', nu - 30 * DAG + 1)];
    expect(filterVerlopen(lijst, '30d', nu).map((d) => d.id)).toEqual(['net-ervoor']);
    expect(filterVerlopen(lijst, '7d', nu)).toEqual([]);
    // Het aantal bij Laatste 10 regelt Rust; hoe oud ook, de leeftijd telt niet.
    const oud = [dictaat('heel-oud', nu - 400 * DAG)];
    expect(filterVerlopen(oud, 'laatste10', nu)).toHaveLength(1);
    // Een tijd ver in de toekomst telt ook hier als verlopen.
    expect(filterVerlopen([dictaat('te-ver', nu + UUR + 1)], 'laatste10', nu)).toEqual([]);
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
