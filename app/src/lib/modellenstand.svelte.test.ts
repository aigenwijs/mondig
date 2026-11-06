import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DownloadItem, RijStand } from './downloadrij';
import type { ModelRegel } from './modellen';

// De feedback rond de Downloaden-knoppen en de zijbalk, met een nep voor de Tauri-commando's.
// De effecten van Svelte draaien hier niet (servermodus); de afgeleide waarden wel.

const kern = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock('@tauri-apps/api/core', () => kern);

import { ModellenStand } from './modellenstand.svelte';

const regel = (id: string): ModelRegel =>
  ({ id, status: 'ontbreekt', daarnaBijHervatten: [] }) as unknown as ModelRegel;

const item = (
  id: number,
  model: string,
  toestand: DownloadItem['toestand'],
  voortgang: DownloadItem['voortgang'] = null
): DownloadItem => ({ id, model, toestand, voortgang });

let volgnummer = 0;
/** Een stand van de rij met een hoger volgnummer dan de vorige, zoals Rust hem stuurt. */
const nieuw = (items: DownloadItem[]): RijStand => ({ versie: ++volgnummer, items });

/** Een belofte die de test zelf laat slagen of mislukken. */
function straks<T>() {
  let klaar!: (waarde: T) => void;
  let mis!: (fout: unknown) => void;
  const belofte = new Promise<T>((ja, nee) => {
    klaar = ja;
    mis = nee;
  });
  return { belofte, klaar, mis };
}

describe('de Downloaden-knop', () => {
  let rij: DownloadItem[];
  let start: ReturnType<typeof straks<number>>;

  beforeEach(() => {
    rij = [];
    start = straks<number>();
    kern.invoke.mockReset();
    kern.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'model_download_start') return start.belofte;
      if (cmd === 'downloads_lijst') return Promise.resolve(nieuw(rij));
      if (cmd === 'modellen_overzicht') return Promise.reject(new Error('niet nodig'));
      return Promise.resolve(null);
    });
  });

  it('zegt meteen na de klik "wordt gestart" en start bij een dubbele klik niets dubbel', async () => {
    const stand = new ModellenStand();
    expect(stand.knopFase('m')).toBe('start');
    const eerste = stand.start(regel('m'), []);
    // Nog voordat Rust antwoordt.
    expect(stand.knopFase('m')).toBe('wordt_gestart');
    const tweede = stand.start(regel('m'), []);
    const starts = kern.invoke.mock.calls.filter(([cmd]) => cmd === 'model_download_start');
    expect(starts).toHaveLength(1);
    rij = [item(1, 'm', { type: 'wacht' })];
    start.klaar(1);
    expect(await eerste).toBeNull();
    expect(await tweede).toBeNull();
    // Daarna de stand uit de rij.
    expect(stand.knopFase('m')).toBe('wacht');
    expect(stand.isActief('m')).toBe(true);
  });

  it('laat een fout bij het starten zien bij het model, en de knop werkt weer', async () => {
    const stand = new ModellenStand();
    const bezig = stand.start(regel('m'), []);
    start.mis({ melding: 'Er is te weinig ruimte op de schijf.', vervolg: 'opnieuw_proberen' });
    const fout = await bezig;
    expect(fout?.melding).toBe('Er is te weinig ruimte op de schijf.');
    expect(stand.foutVoor('m')?.melding).toBe('Er is te weinig ruimte op de schijf.');
    expect(stand.foutVoor('ander')).toBeNull();
    expect(stand.knopFase('m')).toBe('start');
    // Een nieuwe start van hetzelfde model haalt de oude fout weg.
    stand.opRij(nieuw([item(2, 'm', { type: 'bezig' })]));
    expect(stand.foutVoor('m')).toBeNull();
  });
});

describe('de stand van een download', () => {
  it('volgt de rij: wacht, bezig met voortgang, nieuwe poging en weer bezig', () => {
    const stand = new ModellenStand();
    stand.opRij(nieuw([item(1, 'a', { type: 'bezig' }), item(2, 'b', { type: 'wacht' })]));
    expect(stand.knopFase('b')).toBe('wacht');
    stand.opVoortgang({ id: 1, model: 'a', binnen: 25, totaal: 100 });
    expect(stand.knopFase('a')).toBe('bezig');
    expect(stand.procent).toBe(25);
    expect(stand.lopend?.model).toBe('a');
    stand.opRij(
      nieuw([
        item(1, 'a', { type: 'nieuwe_poging', poging: 2, van: 5 }),
        item(2, 'b', { type: 'wacht' })
      ])
    );
    expect(stand.knopFase('a')).toBe('opnieuw');
    // De voortgang die we al hadden, blijft staan tijdens de nieuwe poging.
    expect(stand.procent).toBe(25);
    // Er komen weer bytes binnen: weer gewoon bezig.
    stand.opVoortgang({ id: 1, model: 'a', binnen: 30, totaal: 100 });
    expect(stand.knopFase('a')).toBe('bezig');
    expect(stand.procent).toBe(30);
  });

  it('negeert een oudere stand die na een nieuwere binnenkomt', () => {
    const stand = new ModellenStand();
    const oud = nieuw([item(1, 'a', { type: 'bezig' })]);
    const fout = { type: 'fout', melding: 'Mislukt', vervolg: 'opnieuw_proberen' } as const;
    // Het fout-event komt eerst, het antwoord op downloads_lijst (nog "bezig") daarna.
    stand.opRij(nieuw([item(1, 'a', fout)]));
    stand.opRij(oud);
    expect(stand.itemVoor('a')?.toestand.type).toBe('fout');
    expect(stand.isActief('a')).toBe(false);
    // Een stand met hetzelfde volgnummer telt wel (zelfde wijziging, nieuwere voortgang).
    const zelfde = nieuw([item(1, 'a', fout)]);
    stand.opRij(zelfde);
    stand.opRij({ ...zelfde, items: [item(1, 'a', { type: 'klaar' })] });
    expect(stand.itemVoor('a')?.toestand.type).toBe('klaar');
  });

  it('geeft de fout uit de rij aan het model waar hij bij hoort', () => {
    const stand = new ModellenStand();
    stand.opRij(
      nieuw([
        item(1, 'a', {
          type: 'fout',
          melding: 'De verbinding viel steeds weg.',
          vervolg: 'opnieuw_proberen'
        })
      ])
    );
    expect(stand.foutVoor('a')?.vervolg).toBe('opnieuw_proberen');
    expect(stand.downloadFout?.model).toBe('a');
    expect(stand.knopFase('a')).toBe('start');
    expect(stand.isActief('a')).toBe(false);
  });
});

describe('de zijbalk', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('toont alle downloads; klaar verdwijnt even later, een fout pas als Modellen gezien is', () => {
    const stand = new ModellenStand();
    const fout = { type: 'fout', melding: 'Mislukt', vervolg: 'opnieuw_proberen' } as const;
    stand.opRij(
      nieuw([item(1, 'a', { type: 'bezig' }), item(2, 'b', { type: 'wacht' }), item(3, 'c', fout)])
    );
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['a', 'b', 'c']);
    stand.opRij(
      nieuw([item(1, 'a', { type: 'klaar' }), item(2, 'b', { type: 'bezig' }), item(3, 'c', fout)])
    );
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['a', 'b', 'c']);
    vi.advanceTimersByTime(4000);
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['b', 'c']);
    // De fout blijft, ook na lange tijd, tot de pagina Modellen hem liet zien.
    vi.advanceTimersByTime(60_000);
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['b', 'c']);
    stand.markeerBekeken();
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['b']);
    // Wat nog loopt, blijft altijd staan.
    vi.advanceTimersByTime(60_000);
    expect(stand.zijbalk.map((d) => d.model)).toEqual(['b']);
  });

  it('laat een onderbroken download staan tot Modellen gezien is', () => {
    const stand = new ModellenStand();
    stand.opRij(nieuw([item(4, 'd', { type: 'onderbroken' })]));
    vi.advanceTimersByTime(60_000);
    expect(stand.zijbalk).toHaveLength(1);
    stand.markeerBekeken();
    expect(stand.zijbalk).toHaveLength(0);
    // Hervatten: een nieuw item staat er weer.
    stand.opRij(nieuw([item(5, 'd', { type: 'wacht' })]));
    expect(stand.zijbalk).toHaveLength(1);
  });
});
