import { describe, expect, it, vi } from 'vitest';
import { Afspeler, type AfspeelOmgeving, type AudioElement } from './afspelen.svelte';

// De afspeler met een nep-audio-element: de test bepaalt wanneer het stuk "klaar" is.

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(undefined))
}));

class NepAudio implements AudioElement {
  src = '';
  playbackRate = 1;
  defaultPlaybackRate = 1;
  preservesPitch = false;
  currentTime = 0;
  ended = false;
  speelde = 0;
  #bij: Record<string, (() => void)[]> = {};
  addEventListener(soort: string, bij: () => void): void {
    (this.#bij[soort] ??= []).push(bij);
  }
  removeAttribute(): void {
    this.src = '';
  }
  weiger: Error | null = null;
  play(): Promise<void> {
    if (this.weiger) return Promise.reject(this.weiger);
    this.speelde += 1;
    this.ended = false;
    this.#zeg('play');
    return Promise.resolve();
  }
  pause(): void {
    this.#zeg('pause');
  }
  eindig(): void {
    this.ended = true;
    this.#zeg('ended');
  }
  #zeg(soort: string): void {
    this.#bij[soort]?.forEach((f) => {
      f();
    });
  }
}

function opzet(haal?: (begin: number, eind: number) => Promise<ArrayBuffer>) {
  const audio = new NepAudio();
  const aanvragen: [number, number][] = [];
  const vrijgegeven: string[] = [];
  let teller = 0;
  const omgeving: AfspeelOmgeving = {
    maakAudio: () => audio,
    maakUrl: () => `blob:stuk-${String((teller += 1))}`,
    geefUrlVrij: (url) => vrijgegeven.push(url)
  };
  const afspeler = new Afspeler(
    (begin, eind) => {
      aanvragen.push([begin, eind]);
      return haal ? haal(begin, eind) : Promise.resolve(new ArrayBuffer(8));
    },
    (e) => (e instanceof Error ? e.message : 'Er ging iets mis.'),
    omgeving
  );
  return { audio, afspeler, aanvragen, vrijgegeven };
}

describe('Afspeler', () => {
  it('vraagt het stuk van de regel en speelt het af, met een korte uitloop', async () => {
    const { afspeler, audio, aanvragen } = opzet();
    await afspeler.speel(3, 12_000, 15_000);
    expect(aanvragen).toEqual([[12_000, 15_250]]);
    expect(audio.src).toBe('blob:stuk-1');
    expect(afspeler.speelt).toBe(true);
    expect(afspeler.sleutel).toBe(3);
    expect(afspeler.laden).toBe(false);
    expect(audio.preservesPitch).toBe(true);
  });

  it('pauzeert bij een tweede klik, gaat door bij een derde en vraagt het stuk maar één keer', async () => {
    const { afspeler, audio, aanvragen } = opzet();
    await afspeler.speel(1, 0, 1000);
    await afspeler.speel(1, 0, 1000);
    expect(afspeler.speelt).toBe(false);
    await afspeler.speel(1, 0, 1000);
    expect(afspeler.speelt).toBe(true);
    expect(aanvragen).toHaveLength(1);
    expect(audio.speelde).toBe(2);
  });

  it('begint opnieuw als de regel was afgelopen', async () => {
    const { afspeler, audio } = opzet();
    await afspeler.speel(1, 0, 1000);
    audio.currentTime = 4;
    audio.eindig();
    expect(afspeler.speelt).toBe(false);
    await afspeler.speel(1, 0, 1000);
    expect(audio.currentTime).toBe(0);
    expect(afspeler.speelt).toBe(true);
  });

  it('wisselt naar een andere regel en geeft het vorige stuk vrij', async () => {
    const { afspeler, aanvragen, vrijgegeven } = opzet();
    await afspeler.speel(1, 0, 1000);
    await afspeler.speel(2, 5000, 6000);
    expect(aanvragen).toEqual([
      [0, 1250],
      [5000, 6250]
    ]);
    expect(vrijgegeven).toEqual(['blob:stuk-1']);
    expect(afspeler.sleutel).toBe(2);
  });

  it('spatie (schakel) pauzeert en zet voort, en doet niets zonder regel', async () => {
    const { afspeler } = opzet();
    expect(afspeler.schakel()).toBe(false);
    await afspeler.speel(1, 0, 1000);
    expect(afspeler.schakel()).toBe(true);
    expect(afspeler.speelt).toBe(false);
    expect(afspeler.schakel()).toBe(true);
    expect(afspeler.speelt).toBe(true);
  });

  it('toont de fout in gewone taal en laat niets spelen als het stuk ontbreekt', async () => {
    const { afspeler } = opzet(() => Promise.reject(new Error('De opname is weg.')));
    await afspeler.speel(1, 0, 1000);
    expect(afspeler.fout).toBe('De opname is weg.');
    expect(afspeler.speelt).toBe(false);
    expect(afspeler.sleutel).toBeNull();
    expect(afspeler.laden).toBe(false);
  });

  it('negeert een laat antwoord als de gebruiker intussen een andere regel koos', async () => {
    let los: (bytes: ArrayBuffer) => void = () => undefined;
    const traag = new Promise<ArrayBuffer>((klaar) => {
      los = klaar;
    });
    let aantal = 0;
    const { afspeler, audio } = opzet(() =>
      ++aantal === 1 ? traag : Promise.resolve(new ArrayBuffer(8))
    );
    const eerste = afspeler.speel(1, 0, 1000);
    await afspeler.speel(2, 5000, 6000);
    los(new ArrayBuffer(8));
    await eerste;
    expect(afspeler.sleutel).toBe(2);
    expect(audio.src).toBe('blob:stuk-1');
    expect(afspeler.laden).toBe(false);
  });

  it('zet de snelheid op het element, ook voor een volgend stuk', async () => {
    const { afspeler, audio } = opzet();
    afspeler.zetSnelheid(15);
    await afspeler.speel(1, 0, 1000);
    expect(audio.playbackRate).toBe(1.5);
    expect(audio.defaultPlaybackRate).toBe(1.5);
    afspeler.zetSnelheid(20);
    expect(audio.playbackRate).toBe(2);
    await afspeler.speel(2, 2000, 3000);
    expect(audio.playbackRate).toBe(2);
  });

  it('stopt en geeft het geluid vrij', async () => {
    const { afspeler, vrijgegeven } = opzet();
    await afspeler.speel(1, 0, 1000);
    afspeler.stop();
    expect(afspeler.speelt).toBe(false);
    expect(afspeler.sleutel).toBeNull();
    expect(vrijgegeven).toEqual(['blob:stuk-1']);
  });

  it('speelt het oude stuk niet terug met spatie terwijl een nieuwe regel laadt', async () => {
    let los: (bytes: ArrayBuffer) => void = () => undefined;
    let aantal = 0;
    const { afspeler, audio } = opzet(() =>
      ++aantal === 1
        ? Promise.resolve(new ArrayBuffer(8))
        : new Promise<ArrayBuffer>((klaar) => {
            los = klaar;
          })
    );
    await afspeler.speel(1, 0, 1000);
    const speeldeEerder = audio.speelde;
    const tweede = afspeler.speel(5, 9000, 10_000);
    // Tijdens het laden: spatie doet niets, en het oude stuk is weg.
    expect(afspeler.schakel()).toBe(true);
    expect(audio.speelde).toBe(speeldeEerder);
    expect(audio.src).toBe('');
    los(new ArrayBuffer(8));
    await tweede;
    expect(afspeler.sleutel).toBe(5);
    expect(afspeler.speelt).toBe(true);
  });

  it('negeert de AbortError van een onderbroken play() en toont andere fouten in gewone taal', async () => {
    const { afspeler, audio } = opzet();
    const afgebroken = new Error('The play() request was interrupted');
    afgebroken.name = 'AbortError';
    audio.weiger = afgebroken;
    await afspeler.speel(1, 0, 1000);
    expect(afspeler.fout).toBeNull();
    const nietOndersteund = new Error('no supported source');
    nietOndersteund.name = 'NotSupportedError';
    audio.weiger = nietOndersteund;
    await afspeler.speel(2, 2000, 3000);
    expect(afspeler.fout).toBe('no supported source');
  });
});
