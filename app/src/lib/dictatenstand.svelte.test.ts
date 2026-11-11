import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DictatenAntwoord } from './dictaten';

// De stand van de dictaatgeschiedenis, met een nep voor de Tauri-commando's. Wat telt: de
// juiste commando's met de juiste argumenten, en de lijst ververst na elke actie.

const kern = vi.hoisted(() => ({
  invoke: vi.fn<(cmd: string, args?: unknown) => Promise<unknown>>(),
  isTauri: vi.fn(() => true)
}));
vi.mock('@tauri-apps/api/core', () => kern);

import { DictatenStand } from './dictatenstand.svelte';

const antwoordMet = (
  termijn: DictatenAntwoord['termijn'],
  dictaten: DictatenAntwoord['dictaten'],
  laatste: DictatenAntwoord['laatste'] = null
): DictatenAntwoord => ({
  termijn,
  dictaten,
  totaal: dictaten.length,
  bewaard: dictaten.length,
  laatste
});

const dictaat = (id: string) => ({ id, tijdUnixMs: 1_000, tekst: `tekst ${id}` });

/** `aantal` dictaten van vlak voor nu, nieuwste eerst: d0 is het nieuwste. */
const reeks = (aantal: number) =>
  Array.from({ length: aantal }, (_, i) => ({
    id: `d${String(i)}`,
    tijdUnixMs: Date.now() - i * 1_000,
    tekst: `tekst ${String(i)}`
  }));

function straks<T>() {
  let klaar!: (waarde: T) => void;
  const belofte = new Promise<T>((ja) => {
    klaar = ja;
  });
  return { belofte, klaar };
}

describe('de dictatenstand', () => {
  let antwoord: DictatenAntwoord;

  beforeEach(() => {
    antwoord = antwoordMet('24u', [dictaat('a'), dictaat('b')]);
    kern.invoke.mockReset();
    kern.invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'dictaten_stand' ? antwoord : null)
    );
  });

  it('haalt de termijn en de lijst op bij Rust', async () => {
    const stand = new DictatenStand();
    expect(stand.geladen).toBe(false);
    expect(stand.termijn).toBe('24u');
    antwoord = antwoordMet('7d', [dictaat('x')]);
    await stand.ververs();
    expect(stand.geladen).toBe(true);
    expect(stand.termijn).toBe('7d');
    expect(stand.dictaten.map((d) => d.id)).toEqual(['x']);
  });

  it('verwijderen roept alleen dictaat_verwijder aan; de lijst volgt via het event', async () => {
    const stand0 = new DictatenStand();
    await stand0.ververs();
    kern.invoke.mockClear();
    await stand0.verwijder('a');
    expect(kern.invoke.mock.calls.map((c) => c[0])).toEqual(['dictaat_verwijder']);
    expect(kern.invoke).toHaveBeenCalledWith('dictaat_verwijder', { id: 'a' });
    // Het event komt: dan ververst de stand.
    antwoord = antwoordMet('24u', [dictaat('b')]);
    await stand0.ververs();
    expect(stand0.dictaten.map((d) => d.id)).toEqual(['b']);
  });

  it('alles wissen roept alleen dictaten_wis_alles aan; de lijst volgt via het event', async () => {
    const stand0 = new DictatenStand();
    kern.invoke.mockClear();
    await stand0.wisAlles();
    expect(kern.invoke.mock.calls.map((c) => c[0])).toEqual(['dictaten_wis_alles']);
  });

  it('een andere termijn roept alleen dictaten_bewaren_zet aan met de sleutel', async () => {
    const stand0 = new DictatenStand();
    kern.invoke.mockClear();
    await stand0.zetTermijn('niet');
    expect(kern.invoke.mock.calls).toEqual([['dictaten_bewaren_zet', { termijn: 'niet' }]]);
  });

  it('neemt het aantal bestanden en het laatste dictaat over, en laat verlopen dictaten weg', async () => {
    const stand0 = new DictatenStand();
    const nu = Date.now();
    antwoord = {
      termijn: '24u',
      dictaten: [
        { id: 'vers', tijdUnixMs: nu - 3_600_000, tekst: 'vers' },
        { id: 'oud', tijdUnixMs: nu - 25 * 3_600_000, tekst: 'oud' }
      ],
      totaal: 2,
      bewaard: 2,
      laatste: { tekst: 'vers', id: 'vers' }
    };
    await stand0.ververs();
    expect(stand0.bewaard).toBe(2);
    expect(stand0.laatste?.id).toBe('vers');
    // Rust ruimde nog niet op, de interface laat het oude dictaat toch al weg.
    expect(stand0.zichtbaar.map((d) => d.id)).toEqual(['vers']);
    // De klok tikt door: een uur later is ook het verse dictaat te oud (24 uur + 1 minuut).
    stand0.nu = nu + 23 * 3_600_000 + 61 * 60_000;
    expect(stand0.zichtbaar).toEqual([]);
  });

  it('vraagt eerst de nieuwste tien en neemt het totaal over', async () => {
    const stand = new DictatenStand();
    antwoord = { ...antwoordMet('24u', reeks(10)), totaal: 37, bewaard: 37 };
    await stand.ververs();
    expect(kern.invoke).toHaveBeenCalledWith('dictaten_stand', { begin: 0, limiet: 10 });
    expect(stand.dictaten).toHaveLength(10);
    expect(stand.totaal).toBe(37);
    expect(stand.zichtbaarTotaal).toBe(37);
  });

  it('Meer tonen vraagt er 20 bij en houdt dat vast bij een volgende verversing', async () => {
    const stand = new DictatenStand();
    kern.invoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd !== 'dictaten_stand') return Promise.resolve(null);
      const { begin, limiet } = args as { begin: number; limiet: number };
      return Promise.resolve({
        ...antwoordMet('24u', reeks(37).slice(begin, begin + limiet)),
        totaal: 37,
        bewaard: 37
      });
    });
    await stand.ververs();
    expect(stand.dictaten).toHaveLength(10);
    await stand.meerTonen();
    expect(kern.invoke).toHaveBeenLastCalledWith('dictaten_stand', { begin: 0, limiet: 30 });
    expect(stand.dictaten).toHaveLength(30);
    await stand.meerTonen();
    expect(stand.dictaten).toHaveLength(37);
    expect(stand.gevraagd).toBe(50);
    // Een verversing door een event houdt de uitgeklapte lijst uitgeklapt.
    await stand.ververs();
    expect(stand.dictaten).toHaveLength(37);
  });

  it('een nieuw dictaat bovenaan laat de extra geladen dictaten niet verspringen of verdwijnen', async () => {
    const stand = new DictatenStand();
    let alle = reeks(37);
    kern.invoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd !== 'dictaten_stand') return Promise.resolve(null);
      const { begin, limiet } = args as { begin: number; limiet: number };
      return Promise.resolve({
        ...antwoordMet('24u', alle.slice(begin, begin + limiet)),
        totaal: alle.length,
        bewaard: alle.length
      });
    });
    await stand.ververs();
    await stand.meerTonen();
    expect(stand.dictaten.map((d) => d.id)).toEqual(alle.slice(0, 30).map((d) => d.id));
    // Rust krijgt een nieuw dictaat: het staat nu op plaats 0 en alles schuift een plaats op.
    alle = [{ id: 'nieuw', tijdUnixMs: Date.now(), tekst: 'nieuw' }, ...alle];
    await stand.ververs();
    // Het oudste dat je al zag (nummer 29) staat er nog; er is er een bij gekomen.
    expect(stand.dictaten).toHaveLength(31);
    expect(stand.dictaten[0]?.id).toBe('nieuw');
    expect(stand.dictaten.at(-1)?.id).toBe('d29');
    expect(stand.totaal).toBe(38);
  });

  it('zonder uitgeklapte lijst schuift het oudste er gewoon af', async () => {
    const stand = new DictatenStand();
    let alle = reeks(37);
    kern.invoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd !== 'dictaten_stand') return Promise.resolve(null);
      const { begin, limiet } = args as { begin: number; limiet: number };
      return Promise.resolve({
        ...antwoordMet('24u', alle.slice(begin, begin + limiet)),
        totaal: alle.length,
        bewaard: alle.length
      });
    });
    await stand.ververs();
    alle = [{ id: 'nieuw', tijdUnixMs: Date.now(), tekst: 'nieuw' }, ...alle];
    await stand.ververs();
    expect(stand.dictaten).toHaveLength(10);
    expect(stand.dictaten[0]?.id).toBe('nieuw');
    expect(stand.totaal).toBe(38);
  });

  it('een andere termijn vraagt de verversing door het event al met de basislimiet', async () => {
    const stand = new DictatenStand();
    await stand.meerTonen();
    kern.invoke.mockClear();
    kern.invoke.mockImplementation(async (cmd: string) => {
      // Rust stuurt het event tijdens het commando: de stand ververst voor het klaar is.
      if (cmd === 'dictaten_bewaren_zet') await stand.ververs();
      return cmd === 'dictaten_stand' ? antwoord : null;
    });
    await stand.zetTermijn('laatste10');
    expect(kern.invoke).toHaveBeenCalledWith('dictaten_stand', { begin: 0, limiet: 10 });
    expect(kern.invoke).not.toHaveBeenCalledWith('dictaten_stand', { begin: 0, limiet: 30 });
  });

  it('een mislukte termijn laat de lijst uitgeklapt', async () => {
    const stand = new DictatenStand();
    await stand.meerTonen();
    kern.invoke.mockImplementation((cmd: string) =>
      cmd === 'dictaten_bewaren_zet'
        ? // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
          Promise.reject({ melding: 'mis' })
        : Promise.resolve(antwoord)
    );
    await expect(stand.zetTermijn('niet')).rejects.toEqual({ melding: 'mis' });
    expect(stand.gevraagd).toBe(30);
  });

  it('een andere termijn zet de lijst terug op de eerste tien', async () => {
    const stand = new DictatenStand();
    await stand.meerTonen();
    expect(stand.gevraagd).toBe(30);
    await stand.zetTermijn('laatste10');
    expect(stand.gevraagd).toBe(10);
  });

  it('vraagt aan Rust hoeveel dictaten er wegvallen bij een termijn', async () => {
    const stand = new DictatenStand();
    kern.invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'dictaten_wegval' ? 7 : antwoord)
    );
    expect(await stand.wegval('laatste10')).toBe(7);
    expect(kern.invoke).toHaveBeenCalledWith('dictaten_wegval', { termijn: 'laatste10' });
  });

  it('geeft de fout van Rust door', async () => {
    const stand = new DictatenStand();
    const fout = { melding: 'Mondig kon het dictaat niet wissen.', vervolg: 'geen' };
    kern.invoke.mockImplementation((cmd: string) =>
      // Tauri weigert een commando met een gewone waarde, geen Error.
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      cmd === 'dictaat_verwijder' ? Promise.reject(fout) : Promise.resolve(antwoord)
    );
    await expect(stand.verwijder('a')).rejects.toBe(fout);
  });

  it('negeert een late oudere reactie', async () => {
    const stand = new DictatenStand();
    const oud = straks<DictatenAntwoord>();
    const nieuw = straks<DictatenAntwoord>();
    kern.invoke.mockReturnValueOnce(oud.belofte).mockReturnValueOnce(nieuw.belofte);
    const eerste = stand.ververs();
    const tweede = stand.ververs();
    nieuw.klaar(antwoordMet('7d', [dictaat('nieuw')]));
    await tweede;
    oud.klaar(antwoordMet('24u', [dictaat('oud')]));
    await eerste;
    expect(stand.termijn).toBe('7d');
    expect(stand.dictaten.map((d) => d.id)).toEqual(['nieuw']);
  });

  it('blijft zoals het was als Rust niet antwoordt (gewone browser)', async () => {
    const stand = new DictatenStand();
    kern.invoke.mockRejectedValue(new Error('geen app'));
    await stand.ververs();
    expect(stand.geladen).toBe(false);
    expect(stand.termijn).toBe('24u');
  });
});
