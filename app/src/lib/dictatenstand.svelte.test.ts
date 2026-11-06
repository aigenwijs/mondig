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
): DictatenAntwoord => ({ termijn, dictaten, bewaard: dictaten.length, laatste });

const dictaat = (id: string) => ({ id, tijdUnixMs: 1_000, tekst: `tekst ${id}` });

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
