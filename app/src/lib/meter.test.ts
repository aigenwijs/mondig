import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  HARTSLAG_MS,
  startMeter,
  streepjesAan,
  type MeterEinde,
  type MeterNiveau,
  type MeterOmgeving
} from './meter';

// Een nep-app en een nep-document: genoeg voor startMeter, zonder DOM-omgeving.
function omgeving(opties: { startFout?: unknown; hartslag?: boolean } = {}) {
  let zichtbaar: DocumentVisibilityState = 'visible';
  const doc = new EventTarget();
  const venster = new EventTarget();
  let bijNiveau: ((n: MeterNiveau) => void) | null = null;
  let bijEinde: ((e: MeterEinde) => void) | null = null;
  const losgelaten = vi.fn();
  const omg = {
    start: vi.fn(() =>
      opties.startFout === undefined ? Promise.resolve() : Promise.reject(opties.startFout as Error)
    ),
    hartslag: vi.fn(() => Promise.resolve(opties.hartslag ?? true)),
    stop: vi.fn(() => Promise.resolve()),
    luisterNiveau: vi.fn((bij: (n: MeterNiveau) => void) => {
      bijNiveau = bij;
      return Promise.resolve(losgelaten);
    }),
    luisterEinde: vi.fn((bij: (e: MeterEinde) => void) => {
      bijEinde = bij;
      return Promise.resolve(losgelaten);
    }),
    document: {
      get visibilityState() {
        return zichtbaar;
      },
      addEventListener: doc.addEventListener.bind(doc),
      removeEventListener: doc.removeEventListener.bind(doc)
    },
    venster: {
      addEventListener: venster.addEventListener.bind(venster),
      removeEventListener: venster.removeEventListener.bind(venster)
    }
  } satisfies MeterOmgeving;
  return {
    omg: omg as unknown as MeterOmgeving,
    nep: omg,
    losgelaten,
    zet(stand: DocumentVisibilityState) {
      zichtbaar = stand;
      doc.dispatchEvent(new Event('visibilitychange'));
    },
    verberg() {
      venster.dispatchEvent(new Event('pagehide'));
    },
    niveau(n: MeterNiveau) {
      bijNiveau?.(n);
    },
    einde(e: MeterEinde) {
      bijEinde?.(e);
    }
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('streepjesAan', () => {
  it('zet het niveau om naar 0 tot 10 streepjes', () => {
    expect(streepjesAan(0)).toBe(0);
    expect(streepjesAan(0.5)).toBe(5);
    expect(streepjesAan(0.64)).toBe(6);
    expect(streepjesAan(1)).toBe(10);
    expect(streepjesAan(3)).toBe(10);
    expect(streepjesAan(-1)).toBe(0);
    expect(streepjesAan(Number.NaN)).toBe(0);
  });
});

describe('startMeter', () => {
  it('start de meter, stuurt de hartslag en geeft alleen het niveau van de eigen bron door', async () => {
    const o = omgeving();
    const niveaus: number[] = [];
    const meter = startMeter(
      { bron: 'systeem', apparaat: 'jabra', bijNiveau: (n) => niveaus.push(n) },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(o.nep.start).toHaveBeenCalledWith('systeem', 'jabra');
    o.niveau({ bron: 'systeem', niveau: 0.4 });
    o.niveau({ bron: 'microfoon', niveau: 0.9 });
    expect(niveaus).toEqual([0.4]);
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 3);
    expect(o.nep.hartslag).toHaveBeenCalledTimes(3);
    expect(o.nep.hartslag).toHaveBeenCalledWith('systeem');
    // De hartslag ligt ruim binnen de termijn van de app (5 s).
    expect(HARTSLAG_MS).toBeLessThanOrEqual(2500);
    meter.stop();
  });

  it('stopt bij verbergen of minimaliseren en start weer als het venster terugkomt', async () => {
    const o = omgeving();
    const niveaus: number[] = [];
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: (n) => niveaus.push(n) },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    o.zet('hidden');
    expect(o.nep.stop).toHaveBeenCalledWith('microfoon');
    expect(niveaus.at(-1)).toBe(0);
    // Verborgen: geen hartslag en geen niveaus.
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 4);
    expect(o.nep.hartslag).not.toHaveBeenCalled();
    o.niveau({ bron: 'microfoon', niveau: 0.7 });
    expect(niveaus).not.toContain(0.7);
    o.zet('visible');
    await vi.advanceTimersByTimeAsync(0);
    expect(o.nep.start).toHaveBeenCalledTimes(2);
    meter.stop();
  });

  it('start niet als het venster bij het openen al verborgen is', async () => {
    const o = omgeving();
    o.zet('hidden');
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 2);
    expect(o.nep.start).not.toHaveBeenCalled();
    o.zet('visible');
    await vi.advanceTimersByTimeAsync(0);
    expect(o.nep.start).toHaveBeenCalledOnce();
    meter.stop();
  });

  it('stopt bij pagehide', async () => {
    const o = omgeving();
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    o.verberg();
    expect(o.nep.stop).toHaveBeenCalledOnce();
    meter.stop();
  });

  it('ruimt bij stop alles op en doet daarna niets meer', async () => {
    const o = omgeving();
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    meter.stop();
    expect(o.nep.stop).toHaveBeenCalledOnce();
    expect(o.losgelaten).toHaveBeenCalledTimes(2);
    meter.stop();
    o.zet('hidden');
    o.zet('visible');
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 3);
    expect(o.nep.stop).toHaveBeenCalledOnce();
    expect(o.nep.start).toHaveBeenCalledOnce();
    expect(o.nep.hartslag).not.toHaveBeenCalled();
  });

  it('stopt de meter ook als de component verdwijnt terwijl de bron nog opent', async () => {
    const o = omgeving();
    let klaar: () => void = () => undefined;
    o.nep.start.mockImplementationOnce(
      () =>
        new Promise<void>((r) => {
          klaar = r;
        })
    );
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    meter.stop();
    expect(o.nep.stop).toHaveBeenCalledOnce();
    klaar();
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 3);
    // De trage start start geen hartslag meer.
    expect(o.nep.hartslag).not.toHaveBeenCalled();
  });

  it('meldt een fout bij het starten in gewone taal', async () => {
    const fout = {
      melding: 'Een ander programma gebruikt de microfoon.',
      vervolg: 'opnieuw_proberen'
    };
    const o = omgeving({ startFout: fout });
    const bijFout = vi.fn();
    const meter = startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: () => undefined, bijFout },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(bijFout).toHaveBeenCalledWith(fout);
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 2);
    expect(o.nep.hartslag).not.toHaveBeenCalled();
    meter.stop();
  });

  it('geeft een fout of voorrang door aan de eigenaar en start dan niet zelf opnieuw', async () => {
    const o = omgeving();
    const bijEinde = vi.fn();
    const niveaus: number[] = [];
    startMeter(
      { bron: 'microfoon', apparaat: null, bijNiveau: (n) => niveaus.push(n), bijEinde },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    const einde: MeterEinde = { bron: 'microfoon', reden: 'voorrang', fout: null };
    o.einde({ bron: 'systeem', reden: 'voorrang', fout: null });
    expect(bijEinde).not.toHaveBeenCalled();
    o.einde(einde);
    expect(bijEinde).toHaveBeenCalledWith(einde);
    expect(niveaus.at(-1)).toBe(0);
    // De app stopte hem al: geen eigen stop, geen hartslag, geen herstart.
    expect(o.nep.stop).not.toHaveBeenCalled();
    o.zet('hidden');
    o.zet('visible');
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 3);
    expect(o.nep.start).toHaveBeenCalledOnce();
    expect(o.nep.hartslag).not.toHaveBeenCalled();
  });

  it('start opnieuw na een verlopen hartslag als het venster zichtbaar is', async () => {
    const o = omgeving();
    const meter = startMeter(
      { bron: 'systeem', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(0);
    o.einde({ bron: 'systeem', reden: 'hartslag', fout: null });
    await vi.advanceTimersByTimeAsync(0);
    expect(o.nep.start).toHaveBeenCalledTimes(2);
    meter.stop();
  });

  it('stopt met bevestigen als de app de meter niet meer heeft', async () => {
    const o = omgeving({ hartslag: false });
    const meter = startMeter(
      { bron: 'systeem', apparaat: null, bijNiveau: () => undefined },
      o.omg
    );
    await vi.advanceTimersByTimeAsync(HARTSLAG_MS * 4);
    expect(o.nep.hartslag).toHaveBeenCalledOnce();
    meter.stop();
    // Geen stop naar de app: die heeft de meter al niet meer (en mag een nieuwe niet raken).
    expect(o.nep.stop).not.toHaveBeenCalled();
  });
});
