import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DictatenAntwoord } from '../dictaten';

// De mock moet het gedrag van Rust volgen (mondig-core/src/dictaten.rs en de app in
// app/src-tauri/src/dictaten.rs). De schermen en de screenshots leunen erop: een mock die iets
// anders doet dan Rust laat de interface fouten missen.

const event = vi.hoisted(() => ({ emit: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => event);

import { maakDictatenMock } from './mock-dictaten';

const stand = (m: ReturnType<typeof maakDictatenMock>): DictatenAntwoord =>
  m.handel('dictaten_stand', undefined) as DictatenAntwoord;

describe('de dictatenmock', () => {
  beforeEach(() => {
    event.emit.mockClear();
  });

  it('start met voorbeelden van vandaag en gisteren en vult het geheugen met het nieuwste', () => {
    const m = maakDictatenMock(new Set());
    const { termijn, dictaten } = stand(m);
    expect(termijn).toBe('24u');
    expect(dictaten.length).toBeGreaterThanOrEqual(3);
    const tijden = dictaten.map((d) => d.tijdUnixMs);
    expect(tijden).toEqual([...tijden].sort((a, b) => b - a));
    expect(m.laatste()).toBe(dictaten[0]?.tekst);
  });

  it('dictatenleeg heeft niets, dictatenniet ook niets in het geheugen', () => {
    const leeg = maakDictatenMock(new Set(['dictatenleeg']));
    expect(stand(leeg).dictaten).toEqual([]);
    expect(leeg.laatste()).toBeNull();
    const niet = maakDictatenMock(new Set(['dictatenniet']));
    expect(stand(niet)).toMatchObject({ termijn: 'niet', dictaten: [], bewaard: 0 });
    expect(niet.laatste()).toBeNull();
  });

  it('een nieuw dictaat komt in de lijst en het geheugen en meldt de wijziging', () => {
    const m = maakDictatenMock(new Set(['dictatenleeg']));
    m.nieuw('Een nieuw dictaat.');
    expect(stand(m).dictaten.map((d) => d.tekst)).toEqual(['Een nieuw dictaat.']);
    expect(m.laatste()).toBe('Een nieuw dictaat.');
    expect(event.emit).toHaveBeenCalledWith('dictaten-gewijzigd');
  });

  it('bij Niet bewaren blijft het dictaat alleen in het geheugen', () => {
    const m = maakDictatenMock(new Set(['dictatenniet']));
    m.nieuw('Alleen in het geheugen.');
    expect(stand(m).dictaten).toEqual([]);
    expect(m.laatste()).toBe('Alleen in het geheugen.');
  });

  it('verwijderen van het laatste dictaat vult het geheugen met het nieuwste dat nog staat', () => {
    const m = maakDictatenMock(new Set());
    const [eerste, tweede] = stand(m).dictaten;
    m.handel('dictaat_verwijder', { id: eerste?.id });
    expect(stand(m).dictaten.some((d) => d.id === eerste?.id)).toBe(false);
    expect(m.laatste()).toBe(tweede?.tekst);
    expect(stand(m).laatste?.id).toBe(tweede?.id);
  });

  it('wist op id: twee dictaten met dezelfde tekst raken elkaar niet', () => {
    const m = maakDictatenMock(new Set(['dictatenleeg']));
    m.nieuw('Ja.');
    m.nieuw('Ja.');
    const [nieuwste, oudere] = stand(m).dictaten;
    expect(stand(m).laatste?.id).toBe(nieuwste?.id);
    m.handel('dictaat_verwijder', { id: oudere?.id });
    expect(stand(m).dictaten).toHaveLength(1);
    expect(stand(m).laatste).toEqual({ tekst: 'Ja.', id: nieuwste?.id });
  });

  it('een mislukte schrijfactie laat het dictaat in het geheugen staan, zonder id', () => {
    const m = maakDictatenMock(new Set(['dictatenleeg', 'dictatenschrijffout']));
    m.nieuw('Alleen in het geheugen.');
    expect(stand(m).dictaten).toEqual([]);
    expect(stand(m).laatste).toEqual({ tekst: 'Alleen in het geheugen.', id: null });
  });

  it('meldt het aantal bestanden op schijf', () => {
    const m = maakDictatenMock(new Set());
    expect(stand(m).bewaard).toBe(stand(m).dictaten.length);
  });

  it('weigert een kwaadaardig id', () => {
    const m = maakDictatenMock(new Set());
    const voor = stand(m).dictaten.length;
    for (const id of ['', '../x.json', 'dictaat-..-1.json', 'notities.txt', 'dictaat-1/x.json']) {
      expect(() => m.handel('dictaat_verwijder', { id })).toThrow();
    }
    expect(stand(m).dictaten).toHaveLength(voor);
  });

  it('alles wissen leegt de lijst en het geheugen', () => {
    const m = maakDictatenMock(new Set());
    m.handel('dictaten_wis_alles', undefined);
    expect(stand(m).dictaten).toEqual([]);
    expect(m.laatste()).toBeNull();
  });

  it('naar Niet bewaren wist de lijst maar houdt het laatste dictaat in het geheugen', () => {
    const m = maakDictatenMock(new Set());
    const laatste = m.laatste();
    m.handel('dictaten_bewaren_zet', { termijn: 'niet' });
    expect(stand(m)).toMatchObject({ termijn: 'niet', dictaten: [], bewaard: 0 });
    expect(m.laatste()).toBe(laatste);
  });

  it('korter zetten wist wat ouder is dan de nieuwe termijn', () => {
    const m = maakDictatenMock(new Set(['dictatenweek']));
    const week = stand(m).dictaten.length;
    m.handel('dictaten_bewaren_zet', { termijn: '24u' });
    const dag = stand(m).dictaten;
    expect(dag.length).toBeLessThan(week);
    const grens = Date.now() - 24 * 3_600_000;
    expect(dag.every((d) => d.tijdUnixMs > grens)).toBe(true);
  });

  it('dictatenweek heeft ruim meer dan tien dictaten, en de stand geeft ze in delen', () => {
    const m = maakDictatenMock(new Set(['dictatenweek']));
    const eerste = m.handel('dictaten_stand', undefined) as DictatenAntwoord;
    expect(eerste.dictaten).toHaveLength(10);
    expect(eerste.totaal).toBeGreaterThan(30);
    const volgende = m.handel('dictaten_stand', { begin: 10, limiet: 20 }) as DictatenAntwoord;
    expect(volgende.dictaten).toHaveLength(20);
    expect(volgende.totaal).toBe(eerste.totaal);
    // Aansluitend, nieuwste eerst.
    expect(volgende.dictaten[0]?.tijdUnixMs).toBeLessThan(eerste.dictaten[9]?.tijdUnixMs ?? 0);
  });

  it('Laatste 10 wist direct alles behalve de tien nieuwste en zegt vooraf hoeveel dat is', () => {
    const m = maakDictatenMock(new Set(['dictatenweek']));
    const voor = (m.handel('dictaten_stand', undefined) as DictatenAntwoord).totaal;
    expect(m.handel('dictaten_wegval', { termijn: 'laatste10' })).toBe(voor - 10);
    expect(m.handel('dictaten_wegval', { termijn: '30d' })).toBe(0);
    m.handel('dictaten_bewaren_zet', { termijn: 'laatste10' });
    const na = m.handel('dictaten_stand', { begin: 0, limiet: 50 }) as DictatenAntwoord;
    expect(na).toMatchObject({ termijn: 'laatste10', totaal: 10, bewaard: 10 });
    expect(na.dictaten).toHaveLength(10);
    // Een nieuw dictaat duwt het oudste weg.
    const oudste = na.dictaten[9]?.id;
    m.nieuw('Een elfde.');
    const daarna = m.handel('dictaten_stand', { begin: 0, limiet: 50 }) as DictatenAntwoord;
    expect(daarna.dictaten).toHaveLength(10);
    expect(daarna.dictaten[0]?.tekst).toBe('Een elfde.');
    expect(daarna.dictaten.some((d) => d.id === oudste)).toBe(false);
  });

  it('30 dagen houdt langer vast dan 7 dagen', () => {
    const m = maakDictatenMock(new Set(['dictatenweek']));
    m.handel('dictaten_bewaren_zet', { termijn: '30d' });
    expect(stand(m).termijn).toBe('30d');
    expect(m.handel('dictaten_wegval', { termijn: '24u' })).toBeGreaterThan(0);
  });

  it('weigert een onbekende termijn', () => {
    const m = maakDictatenMock(new Set());
    expect(() => m.handel('dictaten_bewaren_zet', { termijn: '1u' })).toThrow();
    expect(stand(m).termijn).toBe('24u');
  });
});
