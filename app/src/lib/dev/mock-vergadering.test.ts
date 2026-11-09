import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Foutmelding } from '../taak';
import type { BeurtInfo, VergaderingStand } from '../vergadering';

// De mock moet het gedrag van Rust volgen (app/src-tauri/src/vergadering.rs, ADR 0022): sluiten wist
// niets, een verslag uit de lijst opent met alles zoals het was, wissen haalt het weg, en tijdens
// een opname is er niets te openen. De schermen en de screenshots leunen erop.

const event = vi.hoisted(() => ({ emit: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => event);

import { maakVergaderMock, voegBurenSamen } from './mock-vergadering';

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

describe('de vergadermock: sprekers een naam geven (fase 1 verslag-consistent)', () => {
  beforeEach(() => {
    teller = 0;
    vi.stubGlobal('window', globalThis);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const beurtenVan = (stand: VergaderingStand) =>
    stand.toestand.type === 'verslag' ? stand.toestand.beurten : [];
  const stapVan = (stand: VergaderingStand) =>
    stand.toestand.type === 'verslag' ? stand.toestand.sprekerstap : null;

  it('volgt Rust: een naam voor de hele spreker, dezelfde naam voegt samen, en een stap terug', () => {
    const m = maakVergaderMock(new Set(['verslagsprekers']), volgendId);
    const begin = beurtenVan(aanroep(m, 'vergadering_stand'));
    const jij = begin.find((b) => b.kant === 'jij');
    const een = begin.find((b) => b.sprekernummer === 1);
    const twee = begin.find((b) => b.sprekernummer === 2);
    if (!jij || !een || !twee) throw new Error('geen sprekers');
    // Jij wordt Bram: alle beurten van Jij, er is geen "alleen deze beurt" meer.
    let stand = aanroep(m, 'vergadering_naam', { index: jij.index, naam: 'Bram' });
    expect(
      beurtenVan(stand)
        .filter((b) => b.kant === 'jij')
        .map((b) => b.spreker)
    ).toEqual(begin.filter((b) => b.kant === 'jij').map(() => 'Bram'));
    stand = aanroep(m, 'vergadering_naam', { index: een.index, naam: 'Noor' });
    expect(stapVan(stand)?.samengevoegd).toEqual([]);
    // Spreker 2 heet ook Noor: dezelfde persoon, met de naam zoals hij al was.
    stand = aanroep(m, 'vergadering_naam', { index: twee.index, naam: ' noor ' });
    expect(stapVan(stand)).toMatchObject({
      kant: 'anderen',
      samengevoegd: [1, 2],
      naam: 'Noor',
      nummer: 1
    });
    const namen = beurtenVan(stand).map((b) => b.spreker);
    expect(namen).not.toContain('Jij');
    expect(namen).not.toContain('Spreker 1');
    expect(namen).not.toContain('Spreker 2');
    expect(beurtenVan(stand).filter((b) => b.sprekernummer === 2)).toEqual([]);
    // Anderen zonder nummer met de naam Noor: die beurten horen bij Noor.
    const zonder = beurtenVan(stand).find((b) => b.kant === 'anderen' && b.sprekernummer === null);
    if (!zonder) throw new Error('geen beurt zonder spreker');
    stand = aanroep(m, 'vergadering_naam', { index: zonder.index, naam: 'Noor' });
    expect(stapVan(stand)).toMatchObject({ kant: 'anderen', samengevoegd: [1, null] });
    expect(beurtenVan(stand).some((b) => b.kant === 'anderen' && b.sprekernummer === null)).toBe(
      false
    );
    // Eén stap terug, en niet meer.
    const terug = aanroep(m, 'vergadering_stap_terug');
    expect(stapVan(terug)).toBeNull();
    expect(beurtenVan(terug).some((b) => b.sprekernummer === null && b.kant === 'anderen')).toBe(
      true
    );
    expect(fout(m, 'vergadering_stap_terug')?.melding).toBe(
      'Er is niets meer om ongedaan te maken.'
    );
  });

  it('in de ruimte zonder sprekers maakt een alinea de eerste spreker, de rest blijft van niemand', () => {
    const m = maakVergaderMock(new Set(['verslagruimte']), volgendId);
    const begin = beurtenVan(aanroep(m, 'vergadering_stand'));
    // Zoals Rust al deed: alinea's met meer dan drie seconden ertussen, dus niet samengevoegd.
    expect(begin).toHaveLength(4);
    expect(begin.every((b) => b.sprekernummer === null && b.spreker === null)).toBe(true);
    // Een naam geven kan pas na aanwijzen.
    expect(fout(m, 'vergadering_naam', { index: 0, naam: 'Bram' })?.melding).toContain(
      'Wijs deze alinea eerst aan een spreker toe'
    );
    const tweede = begin[1];
    if (!tweede) throw new Error('geen tweede alinea');
    let stand = aanroep(m, 'vergadering_geef_aan_spreker', { index: tweede.index, spreker: null });
    const na = beurtenVan(stand);
    // De andere alinea's blijven zonder spreker: geen "Spreker 1" en geen "Jij".
    expect(na.map((b) => b.sprekernummer)).toEqual([null, 1, null, null]);
    expect(na.map((b) => b.spreker)).toEqual([null, 'Spreker 1', null, null]);
    // De nieuwe spreker een naam geven werkt voor al zijn alinea's.
    stand = aanroep(m, 'vergadering_naam', { index: na[1]?.index, naam: 'Bram' });
    expect(beurtenVan(stand)[1]?.spreker).toBe('Bram');
    // Nog een alinea aan Bram geven kan, en de stap terug zet het terug.
    stand = aanroep(m, 'vergadering_geef_aan_spreker', { index: na[3]?.index, spreker: 1 });
    expect(beurtenVan(stand).map((b) => b.spreker)).toEqual([null, 'Bram', null, 'Bram']);
    stand = aanroep(m, 'vergadering_stap_terug');
    expect(beurtenVan(stand).map((b) => b.spreker)).toEqual([null, 'Bram', null, null]);
  });

  it('een andere wijziging laat de stap terug vervallen', () => {
    const m = maakVergaderMock(new Set(['verslagsprekers']), volgendId);
    const begin = beurtenVan(aanroep(m, 'vergadering_stand'));
    const een = begin.find((b) => b.sprekernummer === 1);
    if (!een) throw new Error('geen spreker');
    const stand = aanroep(m, 'vergadering_geef_aan_spreker', { index: een.index, spreker: 2 });
    expect(stapVan(stand)).not.toBeNull();
    const na = aanroep(m, 'vergadering_zet_tekst', { index: 0, tekst: 'Iets anders.' });
    expect(stapVan(na)).toBeNull();
    expect(fout(m, 'vergadering_stap_terug')).not.toBeNull();
  });
});

describe('de vergadermock: ongedaan maken en notulen volgen Rust', () => {
  beforeEach(() => {
    teller = 0;
    vi.stubGlobal('window', globalThis);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const verslagVan = (stand: VergaderingStand) => {
    if (stand.toestand.type !== 'verslag') throw new Error('geen verslag');
    return stand.toestand;
  };

  it('een wijziging die niets deed of mislukte, laat de stap staan, met hetzelfde nummer', () => {
    const m = maakVergaderMock(new Set(['verslagsprekers']), volgendId);
    const een = verslagVan(aanroep(m, 'vergadering_stand')).beurten.find(
      (b) => b.sprekernummer === 1
    );
    if (!een) throw new Error('geen spreker');
    const stap = verslagVan(
      aanroep(m, 'vergadering_naam', { index: een.index, naam: 'Noor' })
    ).sprekerstap;
    expect(stap).not.toBeNull();
    // Opslaan zonder andere naam, een mislukte naam en een tekst die gelijk blijft.
    let na = aanroep(m, 'vergadering_naam', { index: een.index, naam: 'Noor' });
    expect(verslagVan(na).sprekerstap).toEqual(stap);
    expect(fout(m, 'vergadering_naam', { index: een.index, naam: 'Jij' })?.melding).toContain(
      'Kies een andere naam'
    );
    na = aanroep(m, 'vergadering_zet_tekst', { index: een.index, tekst: een.tekst });
    expect(verslagVan(na).sprekerstap).toEqual(stap);
  });

  it('notulen volgen namen en samengevoegde beurten, en ongedaan maken zet ze terug', () => {
    const m = maakVergaderMock(new Set(['verslagruimtesprekers', 'notulenklaar']), volgendId);
    const begin = verslagVan(aanroep(m, 'vergadering_stand'));
    const info = begin.notulen.info;
    if (!info) throw new Error('geen notulen');
    // Een actiepunt voor Spreker 2, en een besluit met als bron de laatste beurt.
    aanroep(m, 'vergadering_notulen_punt_aanpassen', {
      soort: 'actiepunt',
      id: 1,
      tekst: 'Brief versturen',
      wie: 'Spreker 2',
      wanneer: ''
    });
    expect(info.besluiten[1]?.bron.beurt).toBe(3);
    const twee = begin.beurten.find((b) => b.sprekernummer === 2);
    if (!twee) throw new Error('geen spreker 2');
    // "spreker 1" bij Spreker 2: dezelfde persoon. De alinea's liggen meer dan drie seconden uit
    // elkaar, dus ze blijven apart (Rust voegt alleen buren samen met hooguit `MAX_PAUZE`).
    const na = verslagVan(aanroep(m, 'vergadering_naam', { index: twee.index, naam: 'spreker 1' }));
    expect(na.beurten).toHaveLength(4);
    expect(na.beurten.map((b) => b.sprekernummer)).toEqual([1, 1, 1, 3]);
    expect(na.sprekerstap).toMatchObject({ samengevoegd: [1, 2], naam: null, nummer: 1 });
    expect(na.notulen.info?.besluiten[1]?.bron.beurt).toBe(3);
    expect(na.notulen.info?.actiepunten[0]?.wie).toBe('Spreker 1');
    // Ongedaan maken: de beurten en de notulen zoals ze waren.
    const terug = verslagVan(aanroep(m, 'vergadering_stap_terug'));
    expect(terug.beurten).toHaveLength(4);
    expect(terug.notulen.info?.besluiten[1]?.bron.beurt).toBe(3);
    expect(terug.notulen.info?.actiepunten[0]?.wie).toBe('Spreker 2');
  });

  it('met Vervangen aan tonen Verslag en Notulen hetzelfde als Kopiëren, ook in de naam van een spreker', () => {
    const m = maakVergaderMock(new Set(['verslag', 'notulenklaar']), volgendId);
    const begin = verslagVan(aanroep(m, 'vergadering_stand'));
    expect(begin.notulen.info?.actiepunten[1]?.wie).toBe('Jan');
    // Jan en de uitnodiging zijn gemarkeerd; een beurt krijgt Jan als naam.
    aanroep(m, 'vergadering_persoon_markeer', { waarde: 'Jan' });
    aanroep(m, 'vergadering_persoon_markeer', { waarde: 'speeltuinvereniging' });
    const eerste = begin.beurten.find((b) => b.kant === 'anderen');
    if (!eerste) throw new Error('geen beurt van anderen');
    aanroep(m, 'vergadering_naam', { index: eerste.index, naam: 'Jan' });
    // Vervangen uit: het scherm toont de echte tekst.
    const uit = verslagVan(aanroep(m, 'vergadering_stand'));
    expect(uit.notulen.info?.actiepunten[1]?.wieGetoond).toBe('Jan');
    // Vervangen aan: nergens meer het echte gegeven, en het echte blijft voor bewerken.
    const aan = verslagVan(aanroep(m, 'vergadering_persoon_vervangen', { aan: true }));
    const punt = aan.notulen.info?.actiepunten[1];
    expect(punt?.wieGetoond).toBe('[GEGEVEN]');
    expect(punt?.watGetoond).toBe('Een uitnodiging sturen naar de [GEGEVEN]');
    expect(punt?.wie).toBe('Jan');
    expect(aan.beurten.find((b) => b.index === eerste.index)).toMatchObject({
      spreker: 'Jan',
      sprekerGetoond: '[GEGEVEN]'
    });
  });

  it('met Vervangen aan toont de kop de naam van het verslag zoals de export', () => {
    const m = maakVergaderMock(new Set(['verslag']), volgendId);
    aanroep(m, 'vergadering_verslag_naam', { naam: 'Overleg Jan Jansen' });
    aanroep(m, 'vergadering_persoon_markeer', { waarde: 'Jan Jansen' });
    const uit = verslagVan(aanroep(m, 'vergadering_stand'));
    expect(uit.naamGetoond).toBe('Overleg Jan Jansen');
    const aan = verslagVan(aanroep(m, 'vergadering_persoon_vervangen', { aan: true }));
    expect(aan.naamGetoond).toBe('Overleg [GEGEVEN]');
    // Naam wijzigen begint met de echte naam.
    expect(aan.naam).toBe('Overleg Jan Jansen');
  });

  it('notulen van een opname zonder sprekers hebben geen eigenaar "Jij", ook niet na een spreker', () => {
    const m = maakVergaderMock(new Set(['verslagruimte', 'notulenklaar']), volgendId);
    const begin = verslagVan(aanroep(m, 'vergadering_stand'));
    // Geen spreker: "ik stuur hen een uitnodiging" geeft geen wie, en niets wordt gemarkeerd.
    const punten = begin.notulen.info?.actiepunten ?? [];
    expect(punten.map((a) => a.wie)).toEqual(['', '']);
    expect(punten.some((a) => a.wieNietTeruggevonden)).toBe(false);
    // Een andere alinea aanwijzen verandert de lege "wie" niet in een naam.
    const eerste = begin.beurten[0];
    if (!eerste) throw new Error('geen beurt');
    const na = verslagVan(
      aanroep(m, 'vergadering_geef_aan_spreker', { index: eerste.index, spreker: null })
    );
    expect(na.notulen.info?.actiepunten.map((a) => a.wie)).toEqual(['', '']);
  });

  it('online en bij alleen ik heeft elke regel een label voor de notulen', () => {
    const online = maakVergaderMock(new Set(['verslag', 'notulenklaar']), volgendId);
    const punten = verslagVan(aanroep(online, 'vergadering_stand')).notulen.info?.actiepunten;
    expect(punten?.[1]?.wie).toBe('Jan');
    const alleen = maakVergaderMock(new Set(['verslagalleen', 'notulenklaar']), volgendId);
    const notitie = verslagVan(aanroep(alleen, 'vergadering_stand')).notulen.info?.actiepunten;
    expect(notitie?.[1]?.wie).toBe('Jan');
  });

  it('beurten gaan alleen samen met hooguit drie seconden pauze, zoals Rust', () => {
    const b = (index: number, startMs: number, eindMs: number): BeurtInfo => ({
      index,
      kant: 'anderen',
      sprekernummer: 1,
      spreker: 'Spreker 1',
      sprekerGetoond: 'Spreker 1',
      eigenNaam: false,
      startMs,
      eindMs,
      tekst: `regel ${String(index)}`,
      markeringen: [],
      veranderd: false
    });
    const uit = voegBurenSamen([b(0, 0, 1000), b(1, 2000, 3000), b(2, 10_000, 11_000)]);
    expect(uit.map((r) => [r.startMs, r.eindMs, r.tekst])).toEqual([
      [0, 3000, 'regel 0 regel 1'],
      [10_000, 11_000, 'regel 2']
    ]);
  });
});
