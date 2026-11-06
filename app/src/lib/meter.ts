import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { foutUitCommando, type Foutmelding } from './taak';

// Niveaumeters zonder opname en het testgeluid (fase UX-C): dunne laag om de commando's en
// events van `meter.rs` en `geluid.rs`, plus een helper die een meter start, de hartslag
// stuurt en weer stopt. Er wordt niets bewaard: de app geeft alleen een niveau door.
//
// Privacy: de app stopt een meter zelf als de interface hem niet elke paar seconden
// bevestigt (hartslag, 5 s). De helper stopt hem bovendien meteen als het venster
// verborgen of geminimaliseerd wordt, en start hem weer als het terugkomt.

/** Wat een meter meet: de microfoon, of het geluid dat de computer afspeelt. */
export type Meetbron = 'microfoon' | 'systeem';

/** Het event `meter-niveau`: twintig keer per seconde, 0 tot 1. */
export interface MeterNiveau {
  bron: Meetbron;
  niveau: number;
}

/**
 * Waarom de app een meter zelf stopte: de bron viel uit (`fout`), geen hartslag, een
 * dictaat of opname begon (`voorrang`), of het venster sloot.
 */
export type MeterReden = 'fout' | 'hartslag' | 'voorrang' | 'venster';

/** Het event `meter-einde`. Komt niet na `meterStop`. */
export interface MeterEinde {
  bron: Meetbron;
  reden: MeterReden;
  fout: Foutmelding | null;
}

/**
 * Start de meter van `bron` op `apparaat` (een id uit `microfoonLijst` of
 * `vergaderingAfspeelapparaten`; `null` is de standaard). Een eerdere meter van dezelfde
 * bron stopt. Antwoordt zodra de bron open is.
 */
export function meterStart(bron: Meetbron, apparaat: string | null): Promise<void> {
  return invoke<undefined>('meter_start', { bron, apparaat });
}

/** Bevestigt dat de meter nog in beeld is. `false`: er loopt geen meter (meer). */
export function meterHartslag(bron: Meetbron): Promise<boolean> {
  return invoke<boolean>('meter_hartslag', { bron });
}

/** Stopt de meter van `bron`, of allebei bij `null`. */
export function meterStop(bron: Meetbron | null): Promise<void> {
  return invoke<undefined>('meter_stop', { bron });
}

/**
 * Speelt een zacht testgeluid van ongeveer een seconde op het gekozen afspeelapparaat
 * (`null`: de standaard). Antwoordt als het klaar is.
 */
export function testgeluidSpeel(apparaat: string | null): Promise<void> {
  return invoke<undefined>('testgeluid_speel', { apparaat });
}

const GEEN_EVENTS: Promise<UnlistenFn> = Promise.resolve(() => undefined);

/** Het niveau van de meters, van beide bronnen. */
export function luisterNaarMeterNiveau(bij: (niveau: MeterNiveau) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<MeterNiveau>('meter-niveau', (event) => {
    bij(event.payload);
  });
}

/** De app stopte een meter zelf. */
export function luisterNaarMeterEinde(bij: (einde: MeterEinde) => void): Promise<UnlistenFn> {
  if (!isTauri()) return GEEN_EVENTS;
  return listen<MeterEinde>('meter-einde', (event) => {
    bij(event.payload);
  });
}

/** Zoveel streepjes heeft een meter bij een keuze. */
export const METER_STREEPJES = 10;

/** Zoveel streepjes heeft een meter tijdens het opnemen en bij "Microfoon testen". */
export const METER_STREEPJES_LANG = 20;

/** Hoeveel streepjes (0 tot `aantal`) bij een niveau aan staan. */
export function streepjesAan(niveau: number, aantal: number = METER_STREEPJES): number {
  if (!Number.isFinite(niveau)) return 0;
  return Math.round(Math.min(1, Math.max(0, niveau)) * aantal);
}

/** Zo vaak bevestigt de helper de meter; ruim binnen de 5 s van de app. */
export const HARTSLAG_MS = 1500;

/** Wat `startMeter` nodig heeft van de app en de browser; te vervangen in tests. */
export interface MeterOmgeving {
  start: (bron: Meetbron, apparaat: string | null) => Promise<void>;
  hartslag: (bron: Meetbron) => Promise<boolean>;
  stop: (bron: Meetbron) => Promise<void>;
  luisterNiveau: (bij: (niveau: MeterNiveau) => void) => Promise<UnlistenFn>;
  luisterEinde: (bij: (einde: MeterEinde) => void) => Promise<UnlistenFn>;
  /** Het document, voor `visibilityState` en het event `visibilitychange`. */
  document: Pick<Document, 'visibilityState' | 'addEventListener' | 'removeEventListener'>;
  /** Het venster, voor `pagehide`. */
  venster: Pick<Window, 'addEventListener' | 'removeEventListener'>;
}

/** Opties van `startMeter`. */
export interface MeterOpties {
  bron: Meetbron;
  /** Het gekozen apparaat, of `null` voor de standaard. */
  apparaat: string | null;
  /** Elk nieuw niveau, 0 tot 1. Bij pauze en einde komt er nog een keer 0. */
  bijNiveau: (niveau: number) => void;
  /** Starten lukte niet (bezet, geen apparaat, er loopt een dictaat). */
  bijFout?: (fout: Foutmelding) => void;
  /**
   * De app stopte de meter wegens een fout of voorrang. De helper start hem dan niet zelf
   * opnieuw; dat doet de eigenaar met een nieuwe `startMeter`.
   */
  bijEinde?: (einde: MeterEinde) => void;
}

/** Een lopende meter. */
export interface Meter {
  /** Stopt de meter en ruimt alles op. Daarna doet de meter niets meer. */
  stop: () => void;
}

/** De echte omgeving: de app en het venster. */
function echteOmgeving(): MeterOmgeving {
  return {
    start: meterStart,
    hartslag: meterHartslag,
    stop: (bron) => meterStop(bron),
    luisterNiveau: luisterNaarMeterNiveau,
    luisterEinde: luisterNaarMeterEinde,
    document,
    venster: window
  };
}

/**
 * Start een meter die zichzelf bewaakt: hij stuurt de hartslag, stopt als het venster
 * verborgen of geminimaliseerd wordt en start weer als het terugkomt, en stopt bij
 * `stop()` (roep dat aan als de component verdwijnt, bijvoorbeeld als opruimfunctie van
 * een `$effect`). Gebruik per bron één meter; een andere keuze van het apparaat is een
 * nieuwe meter.
 */
export function startMeter(opties: MeterOpties, omgeving: MeterOmgeving = echteOmgeving()): Meter {
  const { bron, apparaat } = opties;
  // `uit`: gestopt door de eigenaar of door de app (fout, voorrang); dan nooit meer starten.
  let uit = false;
  // Hoort de app nu een meter van ons te hebben (of is die aan het openen)?
  let aan = false;
  // Telt elke (her)start, zodat een trage start die intussen is ingehaald niets meer doet.
  let poging = 0;
  let hartslag: ReturnType<typeof setInterval> | null = null;
  const opruimen: UnlistenFn[] = [];

  const stopHartslag = (): void => {
    if (hartslag !== null) clearInterval(hartslag);
    hartslag = null;
  };

  const pauzeer = (): void => {
    stopHartslag();
    if (aan) {
      aan = false;
      poging += 1;
      void omgeving.stop(bron).catch(() => undefined);
    }
    opties.bijNiveau(0);
  };

  const begin = (): void => {
    if (uit || aan || omgeving.document.visibilityState !== 'visible') return;
    aan = true;
    poging += 1;
    const deze = poging;
    omgeving.start(bron, apparaat).then(
      () => {
        if (deze !== poging) return;
        hartslag = setInterval(() => {
          void omgeving.hartslag(bron).then(
            (loopt) => {
              // De app heeft de meter niet meer (ingehaald door een andere start van deze
              // bron, of gestopt zonder dat het event aankwam): niet blijven bevestigen.
              if (!loopt && deze === poging) {
                stopHartslag();
                aan = false;
                opties.bijNiveau(0);
              }
            },
            () => undefined
          );
        }, HARTSLAG_MS);
      },
      (fout: unknown) => {
        if (deze !== poging) return;
        aan = false;
        opties.bijFout?.(foutUitCommando(fout, 'De meter kon niet starten.'));
      }
    );
  };

  const bijZichtbaarheid = (): void => {
    if (omgeving.document.visibilityState === 'visible') begin();
    else pauzeer();
  };
  const bijPagehide = (): void => {
    pauzeer();
  };

  const stop = (): void => {
    if (uit) return;
    uit = true;
    pauzeer();
    omgeving.document.removeEventListener('visibilitychange', bijZichtbaarheid);
    omgeving.venster.removeEventListener('pagehide', bijPagehide);
    for (const weg of opruimen.splice(0)) weg();
  };

  const houdBij = (luisteraar: Promise<UnlistenFn>): void => {
    void luisteraar.then((weg) => {
      if (uit) weg();
      else opruimen.push(weg);
    });
  };

  houdBij(
    omgeving.luisterNiveau((gemeten) => {
      if (!uit && aan && gemeten.bron === bron) opties.bijNiveau(gemeten.niveau);
    })
  );
  houdBij(
    omgeving.luisterEinde((einde) => {
      if (uit || einde.bron !== bron) return;
      stopHartslag();
      aan = false;
      poging += 1;
      opties.bijNiveau(0);
      // Hartslag of venster: de interface was even weg; starten zodra hij weer zichtbaar is.
      if (einde.reden === 'hartslag' || einde.reden === 'venster') {
        if (omgeving.document.visibilityState === 'visible') begin();
        return;
      }
      stop();
      opties.bijEinde?.(einde);
    })
  );
  omgeving.document.addEventListener('visibilitychange', bijZichtbaarheid);
  omgeving.venster.addEventListener('pagehide', bijPagehide);
  begin();
  return { stop };
}
