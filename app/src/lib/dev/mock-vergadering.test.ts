import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Foutmelding } from '../taak';
import type { VergaderingStand } from '../vergadering';

// De mock moet het gedrag van Rust volgen (app/src-tauri/src/vergadering.rs, ADR 0022): sluiten wist
// niets, een verslag uit de lijst opent met alles zoals het was, wissen haalt het weg, en tijdens
// een opname is er niets te openen. De schermen en de screenshots leunen erop.

const event = vi.hoisted(() => ({ emit: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => event);

import { maakVergaderMock } from './mock-vergadering';

let teller = 0;
const volgendId = (): number => ++teller;

type Mock = ReturnType<typeof maakVergaderMock>;

const aanroep = (m: Mock, cmd: string, args?: Record<string, unknown>): VergaderingStand =>
  m(cmd, args) as VergaderingStand;

/** De fout die een commando gooit, of `null` als het slaagt. */
function fout(m: Mock, cmd: string, args?: Record<string, unknown>): Foutmelding | null {
  try {
    m(cmd, args);
    return null;
  } catch (e) {
    return e as Foutmelding;
  }
}

describe('de vergadermock: Eerder opgenomen', () => {
  beforeEach(() => {
    teller = 0;
    event.emit.mockClear();
    // De mock gebruikt window.setTimeout: in deze omgeving is dat de (nep)klok van het testbestand.
    vi.useFakeTimers();
    vi.stubGlobal('window', globalThis);
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('heeft zonder optie geen bewaarde verslagen', () => {
    expect(aanroep(maakVergaderMock(new Set(), volgendId), 'vergadering_stand').verslagen).toEqual(
      []
    );
  });

  it('eerder geeft negen verslagen, nieuwste eerst', () => {
    const { verslagen } = aanroep(
      maakVergaderMock(new Set(['eerder']), volgendId),
      'vergadering_stand'
    );
    expect(verslagen).toHaveLength(9);
    const begin = verslagen.map((v) => v.beginMs);
    expect(begin).toEqual([...begin].sort((a, b) => b - a));
    expect(
      verslagen.find((v) => v.naam === 'Overleg buurtbijeenkomst speelveld')?.sprekers
    ).toBeGreaterThan(1);
    expect(verslagen.find((v) => v.soort === 'alleen')?.sprekers).toBeNull();
  });

  it('opent een verslag uit de lijst: het verdwijnt uit de lijst en staat open met zijn naam', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    const stand = aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    expect(stand.verslagen.map((v) => v.id)).not.toContain('eerder-1');
    expect(stand.verslagen).toHaveLength(8);
    expect(stand.toestand).toMatchObject({
      type: 'verslag',
      naam: 'Overleg buurtbijeenkomst speelveld',
      soort: 'online'
    });
    if (stand.toestand.type !== 'verslag') return;
    expect(stand.toestand.beurten.length).toBeGreaterThan(0);
    expect(stand.toestand.wistMs).not.toBeNull();
    // De sprekers zijn herkend, zoals de lijst zei.
    expect(stand.toestand.beurten.some((b) => b.sprekernummer !== null)).toBe(true);
  });

  it('weigert een verslag dat er niet is', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    expect(fout(m, 'vergadering_verslag_openen', { id: 'bestaat-niet' })?.melding).toBe(
      'Dit verslag staat er niet meer.'
    );
  });

  it('weigert te openen tijdens een opname en laat de lijst staan', () => {
    const m = maakVergaderMock(new Set(['eerder', 'opname']), volgendId);
    const melding = fout(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    expect(melding?.melding).toContain('Er loopt een opname');
    const stand = aanroep(m, 'vergadering_stand');
    expect(stand.toestand.type).toBe('neemtOp');
    expect(stand.verslagen).toHaveLength(9);
  });

  it('weigert te openen tijdens het uitschrijven', () => {
    const m = maakVergaderMock(new Set(['eerder', 'uitschrijven']), volgendId);
    vi.advanceTimersByTime(1);
    expect(aanroep(m, 'vergadering_stand').toestand.type).toBe('uitschrijven');
    expect(fout(m, 'vergadering_verslag_openen', { id: 'eerder-1' })?.melding).toContain(
      'schrijft nog een vergadering uit'
    );
  });

  it('sluiten wist niets: het verslag staat weer in de lijst en is weer te openen', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-2' });
    const stand = aanroep(m, 'vergadering_sluiten');
    expect(stand.toestand.type).toBe('klaar');
    expect(stand.verslagen).toHaveLength(9);
    expect(stand.verslagen.map((v) => v.id)).toContain('eerder-2');
    expect(aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-2' }).toestand.type).toBe(
      'verslag'
    );
  });

  it('een ander verslag openen sluit het huidige, zonder te wissen', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    const stand = aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-3' });
    expect(stand.verslagen).toHaveLength(8);
    expect(stand.verslagen.map((v) => v.id)).toContain('eerder-1');
    expect(stand.verslagen.map((v) => v.id)).not.toContain('eerder-3');
    expect(stand.toestand).toMatchObject({ type: 'verslag', soort: 'ruimte' });
  });

  it('wist een verslag uit de lijst', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    const stand = aanroep(m, 'vergadering_verslag_wissen', { id: 'eerder-4' });
    expect(stand.verslagen).toHaveLength(8);
    expect(stand.verslagen.map((v) => v.id)).not.toContain('eerder-4');
    expect(fout(m, 'vergadering_verslag_wissen', { id: 'eerder-4' })).not.toBeNull();
  });

  it('wist het open verslag: terug naar klaar en het staat niet meer in de lijst', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    const stand = aanroep(m, 'vergadering_verslag_wissen', { id: null });
    expect(stand.toestand.type).toBe('klaar');
    expect(stand.verslagen).toHaveLength(8);
    expect(stand.verslagen.map((v) => v.id)).not.toContain('eerder-1');
    // Zonder open verslag valt er niets te wissen.
    expect(fout(m, 'vergadering_verslag_wissen', { id: null })).not.toBeNull();
  });

  it('geeft het verslag een naam, schoon en begrensd, en een lege naam zet hem terug', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-2' });
    let stand = aanroep(m, 'vergadering_verslag_naam', {
      naam: '  Mijn   notitie \n over speelveld '
    });
    expect(stand.toestand).toMatchObject({ naam: 'Mijn notitie over speelveld' });
    stand = aanroep(m, 'vergadering_verslag_naam', { naam: 'x'.repeat(250) });
    expect((stand.toestand as { naam: string }).naam).toHaveLength(100);
    aanroep(m, 'vergadering_verslag_naam', { naam: 'Terug te vinden' });
    // De naam is de titel in de lijst en blijft na heropenen.
    stand = aanroep(m, 'vergadering_sluiten');
    expect(stand.verslagen.find((v) => v.id === 'eerder-2')?.naam).toBe('Terug te vinden');
    stand = aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-2' });
    expect(stand.toestand).toMatchObject({ naam: 'Terug te vinden' });
    stand = aanroep(m, 'vergadering_verslag_naam', { naam: '   ' });
    expect((stand.toestand as { naam: string | null }).naam).toBeNull();
  });

  it('een naam geven kan alleen bij een open verslag', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    expect(fout(m, 'vergadering_verslag_naam', { naam: 'Naam' })?.melding).toBe(
      'Er is geen verslag open.'
    );
  });

  it('bewaart de markeringen van persoonsgegevens bij het verslag', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    aanroep(m, 'vergadering_persoon_vervangen', { aan: true });
    aanroep(m, 'vergadering_sluiten');
    // Een ander verslag begint niet met de markeringen van het vorige.
    const ander = aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-3' });
    expect((ander.toestand as { persoon: { vervangen: boolean } }).persoon.vervangen).toBe(false);
    aanroep(m, 'vergadering_sluiten');
    const terug = aanroep(m, 'vergadering_verslag_openen', { id: 'eerder-1' });
    expect((terug.toestand as { persoon: { vervangen: boolean } }).persoon.vervangen).toBe(true);
  });

  it('volgt de bewaartermijn uit Instellingen in de wistijd', () => {
    const m = maakVergaderMock(new Set(['eerder']), volgendId);
    const voor = aanroep(m, 'vergadering_stand').verslagen.find((v) => v.id === 'eerder-1');
    m('bewaartermijn_zet', { dagen: 30 });
    const na = aanroep(m, 'vergadering_stand').verslagen.find((v) => v.id === 'eerder-1');
    expect((na?.wistMs ?? 0) - (voor?.wistMs ?? 0)).toBe(23 * 86_400_000);
  });

  it('sluiten zonder open verslag geeft een melding in gewone taal', () => {
    const m = maakVergaderMock(new Set(), volgendId);
    expect(fout(m, 'vergadering_sluiten')?.melding).toBe('Er is geen verslag open.');
  });

  it('weigert te sluiten als het verslag niet bewaard is, tenzij je toch sluit', () => {
    const m = maakVergaderMock(new Set(['verslag', 'bewaarfout']), volgendId);
    const stand = aanroep(m, 'vergadering_stand');
    expect(stand.toestand).toMatchObject({ type: 'verslag', bewarenMislukt: true });
    expect(stand.fout?.melding).toContain('niet bewaren');
    expect(fout(m, 'vergadering_sluiten', { toch: false })?.melding).toContain('sluit toch');
    expect(aanroep(m, 'vergadering_stand').toestand.type).toBe('verslag');
    // Toch sluiten kan, en de melding verdwijnt dan niet stil.
    const na = aanroep(m, 'vergadering_sluiten', { toch: true });
    expect(na.toestand.type).toBe('klaar');
    expect(na.fout).not.toBeNull();
    expect(na.verslagen).toHaveLength(1);
  });

  it('een gewoon verslag heeft geen bewaarfout en sluit zonder toch', () => {
    const m = maakVergaderMock(new Set(['verslag']), volgendId);
    expect(aanroep(m, 'vergadering_stand').toestand).toMatchObject({ bewarenMislukt: false });
    expect(aanroep(m, 'vergadering_sluiten', {}).toestand.type).toBe('klaar');
  });
});

describe('de vergadermock: opnieuw uitschrijven (N-C4)', () => {
  beforeEach(() => {
    teller = 0;
    event.emit.mockClear();
    vi.useFakeTimers();
    vi.stubGlobal('window', globalThis);
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('volgt Rust: weigeren tijdens het lopen, daarna de melding en terugzetten', async () => {
    const m = maakVergaderMock(new Set(['verslag', 'snelmodel', 'verbeterd']), volgendId);
    const begin = aanroep(m, 'vergadering_stand');
    expect(begin.toestand.type === 'verslag' && begin.toestand.verbeterd).toBe(2);
    const laden = m('vergadering_opnieuw_keuzes', undefined);
    vi.advanceTimersByTime(200);
    const keuzes = (await laden) as {
      keuzes: { modelId: string; aanwezig: boolean }[];
    };
    expect(keuzes.keuzes.map((k) => [k.modelId, k.aanwezig])).toEqual([
      ['canary-1b-v2-q8', true],
      ['whisper-large-v3-turbo-q8', false]
    ]);
    expect(fout(m, 'vergadering_opnieuw_start', { model: 'whisper-large-v3-turbo-q8' })).not.toBe(
      null
    );
    const loopt = aanroep(m, 'vergadering_opnieuw_start', { model: 'canary-1b-v2-q8' });
    expect(loopt.toestand.type === 'verslag' && loopt.toestand.opnieuw?.rol).toBe('nauwkeurig');
    expect(fout(m, 'vergadering_zet_tekst', { index: 0, tekst: 'Iets.' })?.melding).toContain(
      'Verbeteren kan weer'
    );
    expect(fout(m, 'vergadering_sluiten', { toch: true })).not.toBe(null);
    vi.advanceTimersByTime(11_000);
    const klaar = aanroep(m, 'vergadering_stand');
    if (klaar.toestand.type !== 'verslag') throw new Error('geen verslag');
    expect(klaar.toestand.opnieuw).toBeNull();
    expect(klaar.toestand.naOpnieuw?.veranderd).toBeGreaterThan(0);
    expect(klaar.toestand.vorigeVersie).toBe(true);
    expect(klaar.toestand.modelId).toBe('canary-1b-v2-q8');
    expect(klaar.toestand.beurten.some((b) => b.veranderd)).toBe(true);
    const terug = aanroep(m, 'vergadering_vorige_terugzetten');
    if (terug.toestand.type !== 'verslag') throw new Error('geen verslag');
    expect(terug.toestand.modelId).toBe('parakeet-ultra-q8');
    expect(terug.toestand.vorigeVersie).toBe(false);
    expect(terug.toestand.verbeterd).toBe(2);
    expect(terug.toestand.beurten.some((b) => b.veranderd)).toBe(false);
  });

  it('Stoppen laat het verslag staan en een gewiste opname kan niet opnieuw', () => {
    const m = maakVergaderMock(new Set(['opnieuwbezig', 'verslag']), volgendId);
    const stop = aanroep(m, 'vergadering_opnieuw_stoppen');
    expect(stop.toestand.type === 'verslag' && stop.toestand.opnieuw).toBeNull();
    expect(stop.toestand.type === 'verslag' && stop.toestand.vorigeVersie).toBe(false);
    const gewist = aanroep(
      maakVergaderMock(new Set(['verslag', 'gewist']), volgendId),
      'vergadering_stand'
    );
    expect(gewist.toestand.type === 'verslag' && gewist.toestand.opnameAanwezig).toBe(false);
  });
});
