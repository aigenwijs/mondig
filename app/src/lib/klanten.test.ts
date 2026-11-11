import { beforeEach, describe, expect, it, vi } from 'vitest';

const kern = vi.hoisted(() => ({
  invoke: vi.fn((): Promise<unknown> => Promise.resolve(null)),
  isTauri: vi.fn(() => false)
}));
vi.mock('@tauri-apps/api/core', () => kern);

import {
  comboToets,
  filterKlanten,
  klantenLijst,
  klantenVerwijder,
  MAX_SUGGESTIES,
  volgendeActief,
  type ComboStand
} from './klanten';

// De klantnamen (UI-E): de commando's, het filter met het gevonden stuk, en de toetsen van de
// suggestielijst volgens het patroon "combobox with list autocomplete" (WAI-ARIA APG).

const namen = [
  'Gemeente Vlaardingen',
  'Gemeente Meerwijk',
  'Provincie Zuid-Holland',
  'Waterschap Hollandse Delta',
  'Stichting Gemeenschapszin'
];

describe("de commando's voor klantnamen", () => {
  beforeEach(() => {
    kern.invoke.mockClear();
  });

  it('haalt de lijst op en verwijdert hem met één commando', async () => {
    kern.invoke.mockResolvedValueOnce(['Gemeente Vlaardingen']);
    expect(await klantenLijst()).toEqual(['Gemeente Vlaardingen']);
    expect(kern.invoke).toHaveBeenLastCalledWith('klanten_lijst');
    await klantenVerwijder();
    expect(kern.invoke).toHaveBeenLastCalledWith('klanten_verwijder');
  });
});

describe('de suggesties bij wat er getypt is', () => {
  it('zoekt zonder verschil in hoofdletters en markeert het gevonden stuk', () => {
    const [eerste] = filterKlanten(namen, 'gem');
    expect(eerste).toEqual({
      naam: 'Gemeente Vlaardingen',
      voor: '',
      gevonden: 'Gem',
      na: 'eente Vlaardingen'
    });
    // De schrijfwijze van de lijst blijft, ook als je met kleine letters typte.
    expect(filterKlanten(namen, 'GEM').map((s) => s.naam)).toContain('Gemeente Meerwijk');
  });

  it('vindt de tekst ook midden in een naam, na de namen die ermee beginnen', () => {
    const uit = filterKlanten(namen, 'gem');
    expect(uit.map((s) => s.naam)).toEqual([
      'Gemeente Vlaardingen',
      'Gemeente Meerwijk',
      'Stichting Gemeenschapszin'
    ]);
    const midden = filterKlanten(namen, 'delta');
    expect(midden).toEqual([
      {
        naam: 'Waterschap Hollandse Delta',
        voor: 'Waterschap Hollandse ',
        gevonden: 'Delta',
        na: ''
      }
    ]);
    // Staat het midden in de naam, dan komen namen die ermee beginnen eerst.
    const volgorde = filterKlanten(['Oude Gemeente', 'Gemeente Nieuw'], 'gemeente');
    expect(volgorde.map((s) => s.naam)).toEqual(['Gemeente Nieuw', 'Oude Gemeente']);
  });

  it('toont hooguit zes suggesties, de nieuwste eerst', () => {
    const veel = Array.from({ length: 20 }, (_, i) => `Klant ${String(i)}`);
    expect(MAX_SUGGESTIES).toBe(6);
    expect(filterKlanten(veel, 'klant').map((s) => s.naam)).toEqual(veel.slice(0, 6));
    // Zonder tekst: de nieuwste namen, zonder markering.
    expect(filterKlanten(veel, '')).toHaveLength(6);
    expect(filterKlanten(veel, '  ')[0]).toEqual({
      naam: 'Klant 0',
      voor: '',
      gevonden: '',
      na: 'Klant 0'
    });
  });

  it('geeft niets terug bij een tekst die nergens in voorkomt, of als de naam er al helemaal staat', () => {
    expect(filterKlanten(namen, 'xyz')).toEqual([]);
    expect(filterKlanten(namen, 'Gemeente Meerwijk')).toEqual([]);
    expect(filterKlanten([], 'gem')).toEqual([]);
  });

  it('laat het gevonden stuk weg als kleinmaken de lengte verandert', () => {
    // "İ" wordt bij kleinmaken twee tekens; dan zou de markering verschuiven.
    const uit = filterKlanten(['İstanbul Haven'], 'haven');
    expect(uit).toEqual([{ naam: 'İstanbul Haven', voor: 'İstanbul Haven', gevonden: '', na: '' }]);
  });
});

describe('de pijltjes in de lijst', () => {
  it('gaan omlaag en omhoog en lopen rond aan de randen', () => {
    expect(volgendeActief(-1, 1, 3)).toBe(0);
    expect(volgendeActief(-1, -1, 3)).toBe(2);
    expect(volgendeActief(0, 1, 3)).toBe(1);
    expect(volgendeActief(2, 1, 3)).toBe(0);
    expect(volgendeActief(0, -1, 3)).toBe(2);
    expect(volgendeActief(0, 1, 0)).toBe(-1);
  });
});

describe('de toetsen van het veld Klant (APG: combobox met lijst)', () => {
  const dicht: ComboStand = { open: false, actief: -1 };

  it('ArrowDown opent de lijst en kiest de eerste, ArrowUp de laatste', () => {
    const omlaag = comboToets(dicht, 'ArrowDown', 3);
    expect(omlaag.stand).toEqual({ open: true, actief: 0 });
    // De cursor mag niet in het veld springen.
    expect(omlaag.blokkeer).toBe(true);
    expect(comboToets(dicht, 'ArrowUp', 3).stand).toEqual({ open: true, actief: 2 });
  });

  it('de pijltjes lopen door de open lijst en rond aan de randen', () => {
    let stand: ComboStand = { open: true, actief: -1 };
    stand = comboToets(stand, 'ArrowDown', 2).stand;
    expect(stand.actief).toBe(0);
    stand = comboToets(stand, 'ArrowDown', 2).stand;
    expect(stand.actief).toBe(1);
    stand = comboToets(stand, 'ArrowDown', 2).stand;
    expect(stand.actief).toBe(0);
    stand = comboToets(stand, 'ArrowUp', 2).stand;
    expect(stand.actief).toBe(1);
  });

  it('doet niets met de pijltjes zonder suggesties', () => {
    const uit = comboToets(dicht, 'ArrowDown', 0);
    expect(uit.stand).toEqual(dicht);
    expect(uit.blokkeer).toBe(false);
  });

  it('Enter neemt de gekozen suggestie over en sluit de lijst', () => {
    const uit = comboToets({ open: true, actief: 1 }, 'Enter', 3);
    expect(uit.kies).toBe(1);
    expect(uit.klaar).toBe(false);
    expect(uit.stand).toEqual(dicht);
    expect(uit.blokkeer).toBe(true);
  });

  it('Enter zonder gekozen suggestie bewaart de getypte tekst: vrije tekst mag', () => {
    // Open maar niets aangewezen: er wordt niet vanzelf de eerste suggestie gekozen.
    const open = comboToets({ open: true, actief: -1 }, 'Enter', 3);
    expect(open.kies).toBeNull();
    expect(open.klaar).toBe(true);
    expect(open.stand).toEqual(dicht);
    // Dicht, of zonder suggesties: hetzelfde.
    expect(comboToets(dicht, 'Enter', 3)).toMatchObject({ kies: null, klaar: true });
    expect(comboToets({ open: true, actief: 0 }, 'Enter', 0)).toMatchObject({
      kies: null,
      klaar: true
    });
  });

  it('Escape sluit alleen de lijst en geeft de toets niet door aan wat eromheen open staat', () => {
    const uit = comboToets({ open: true, actief: 1 }, 'Escape', 3);
    expect(uit.stand).toEqual(dicht);
    expect(uit.blokkeer).toBe(true);
    expect(uit.stopVerder).toBe(true);
    // Staat de lijst dicht, dan is Escape niet van dit veld.
    const dichtUit = comboToets(dicht, 'Escape', 3);
    expect(dichtUit.stopVerder).toBe(false);
    expect(dichtUit.blokkeer).toBe(false);
  });

  it('Tab sluit de lijst zonder iets te kiezen', () => {
    const uit = comboToets({ open: true, actief: 1 }, 'Tab', 3);
    expect(uit.stand).toEqual(dicht);
    expect(uit.kies).toBeNull();
    expect(uit.blokkeer).toBe(false);
  });

  it('laat andere toetsen met rust', () => {
    const stand: ComboStand = { open: true, actief: 1 };
    const uit = comboToets(stand, 'a', 3);
    expect(uit.stand).toBe(stand);
    expect(uit).toMatchObject({ kies: null, klaar: false, blokkeer: false, stopVerder: false });
  });
});
