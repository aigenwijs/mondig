import {
  METER_STREEPJES_LANG,
  startMeter,
  streepjesAan,
  type Meter,
  type MeterOpties
} from './meter';
import type { Foutmelding } from './taak';

// "Microfoon testen" op Dicteren (N-C1, besluit 3). De microfoon en de meter gaan pas aan na
// een klik, en weer uit bij "Klaar", na 15 seconden stilte en als het scherm verdwijnt.
// Openen van Dicteren zet de microfoon dus niet aan. Deze module regelt dat; de component
// tekent alleen. Alles wat de klok of de meter nodig heeft is te vervangen, voor de tests.

/** Zo lang mag het stil zijn voordat de test zichzelf uitzet. */
export const MICTEST_STILTE_MS = 15_000;

/** Onder dit niveau (0 tot 1) telt het als stil: ruis van de microfoon, geen stem. */
export const MICTEST_STILTE_NIVEAU = 0.04;

/** Waarom de test niet meer loopt. */
export type MicTestEinde = 'klaar' | 'stilte' | 'voorrang';

/** Wat de component laat zien. */
export interface MicTestStand {
  /** Loopt de test (microfoon en meter aan)? */
  aan: boolean;
  /** Hoeveel streepjes aan staan (van `aantal`, standaard twintig). */
  streepjes: number;
  /** Bezet, geen toegang of uitgevallen: een melding in gewone taal. */
  fout: Foutmelding | null;
  /** Waarom de test voor het laatst stopte, of `null` als hij loopt of nog niet liep. */
  einde: MicTestEinde | null;
}

/** Wat de test van de klok en de meter nodig heeft. */
export interface MicTestOmgeving {
  startMeter: (opties: MeterOpties) => Meter;
  setTimeout: (functie: () => void, ms: number) => unknown;
  clearTimeout: (id: unknown) => void;
}

const ECHTE_OMGEVING: MicTestOmgeving = {
  startMeter: (opties) => startMeter(opties),
  setTimeout: (functie, ms) => globalThis.setTimeout(functie, ms),
  clearTimeout: (id) => {
    globalThis.clearTimeout(id as ReturnType<typeof setTimeout>);
  }
};

export const BEGINSTAND: MicTestStand = { aan: false, streepjes: 0, fout: null, einde: null };

export interface MicTest {
  /** Zet de microfoon en de meter aan. Een test die al loopt blijft zoals hij is. */
  begin: () => void;
  /** "Klaar": zet alles uit. */
  klaar: () => void;
  /** Het scherm verdwijnt: zet alles uit, zonder nog een stand te melden. */
  sluit: () => void;
}

/**
 * Maakt een microfoontest. `apparaat` wordt gelezen bij elke start (de gekozen microfoon, of
 * `null` voor de standaard); `bijStand` krijgt elke nieuwe stand. `aantal` is het aantal
 * streepjes van de meter: twintig op Dicteren, tien bij de keuzelijst in Instellingen.
 */
export function maakMicTest(
  apparaat: () => string | null,
  bijStand: (stand: MicTestStand) => void,
  omgeving: MicTestOmgeving = ECHTE_OMGEVING,
  aantal: number = METER_STREEPJES_LANG
): MicTest {
  let stand: MicTestStand = BEGINSTAND;
  let meter: Meter | null = null;
  let stilteTimer: unknown = null;
  let weg = false;

  const zet = (nieuw: Partial<MicTestStand>): void => {
    stand = { ...stand, ...nieuw };
    if (!weg) bijStand(stand);
  };

  const wisTimer = (): void => {
    if (stilteTimer !== null) omgeving.clearTimeout(stilteTimer);
    stilteTimer = null;
  };

  const stopAlles = (): void => {
    wisTimer();
    const lopend = meter;
    meter = null;
    lopend?.stop();
  };

  const stopMet = (einde: MicTestEinde): void => {
    stopAlles();
    zet({ aan: false, streepjes: 0, einde });
  };

  const startStilteTimer = (): void => {
    wisTimer();
    stilteTimer = omgeving.setTimeout(() => {
      stilteTimer = null;
      stopMet('stilte');
    }, MICTEST_STILTE_MS);
  };

  return {
    begin() {
      if (weg || meter !== null) return;
      zet({ aan: true, streepjes: 0, fout: null, einde: null });
      startStilteTimer();
      meter = omgeving.startMeter({
        bron: 'microfoon',
        apparaat: apparaat(),
        bijNiveau: (niveau) => {
          if (meter === null) return;
          // Geluid dat nergens op lijkt (ruis) houdt de microfoon niet aan.
          if (niveau >= MICTEST_STILTE_NIVEAU) startStilteTimer();
          zet({ streepjes: streepjesAan(niveau, aantal) });
        },
        bijFout: (fout) => {
          // De microfoon ging niet open (bezet, geen toegang): de melding staat in de regel
          // en de test hoeft niet langer te lopen. De meter moet echt stoppen: anders blijft
          // zijn zichtbaarheidsluisteraar staan en gaat de microfoon na minimaliseren en
          // terugkomen stilletjes weer aan.
          stopAlles();
          zet({ fout, streepjes: 0 });
        },
        bijEinde: (einde) => {
          // De app stopte de meter zelf: een dictaat of opname gaat voor, of de microfoon viel uit.
          stopAlles();
          if (einde.fout) zet({ fout: einde.fout, streepjes: 0 });
          else zet({ aan: false, streepjes: 0, einde: 'voorrang' });
        }
      });
    },
    klaar() {
      if (weg) return;
      stopMet('klaar');
    },
    sluit() {
      weg = true;
      stopAlles();
    }
  };
}
