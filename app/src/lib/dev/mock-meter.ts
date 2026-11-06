/* eslint-disable @typescript-eslint/only-throw-error -- Tauri weigert een commando met een foutmelding, geen Error */
// De niveaumeters zonder opname en het testgeluid (fase UX-C) in de dev-mock, zodat de
// schermen van UX-D in de gewone browser na te lopen zijn. Zelfde vorm als Rust (`meter.rs`):
// `meter-niveau` twintig keer per seconde, `meter-einde` als de "app" de meter stopt, ook
// als de hartslag 5 s uitblijft.
//
// Scenario's (`?mock=`, naast de andere opties):
//   meterfout:    de microfoon is bezet; meter_start van de microfoon weigert
//   metergeenapp: het systeemgeluid kan hier niet (zoals op macOS en Linux nu nog)
//   meterstil:    de microfoon hoort nooit iets
//   metermuziek:  de computer speelt al iets af, dus de meter van het systeemgeluid beweegt
//   meteruitval:  de microfoon valt na 4 s uit (headset weg)
//   testgeluidfout: het testgeluid speelt niet af
// Zonder optie praat iemand in de microfoon en is het systeemgeluid stil, behalve tijdens
// het testgeluid (ongeveer zes van de tien streepjes, 0,9 s).

import type { InvokeArgs } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { Meetbron, MeterEinde, MeterNiveau, MeterReden } from '../meter';
import type { Foutmelding } from '../taak';
import { ONBEKEND } from './mock-bestanden';

const TIK_MS = 50;
const HARTSLAG_TERMIJN_MS = 5000;
const TESTGELUID_MS = 900;

interface Lopend {
  timer: number;
  begin: number;
  laatsteHartslag: number;
}

function tekstOfNull(args: InvokeArgs | undefined, naam: string): string | null {
  if (
    args &&
    !Array.isArray(args) &&
    !(args instanceof ArrayBuffer) &&
    !(args instanceof Uint8Array)
  ) {
    const waarde = args[naam];
    if (typeof waarde === 'string') return waarde;
  }
  return null;
}

function bronUit(args: InvokeArgs | undefined): Meetbron | null {
  const bron = tekstOfNull(args, 'bron');
  return bron === 'microfoon' || bron === 'systeem' ? bron : null;
}

export function maakMeterMock(
  opties: Set<string>
): (cmd: string, args: InvokeArgs | undefined) => unknown {
  const lopend = new Map<Meetbron, Lopend>();
  // Tot wanneer het testgeluid speelt (ms sinds 1970), 0 als het niet speelt.
  let testgeluidTot = 0;

  const meldEinde = (bron: Meetbron, reden: MeterReden, fout: Foutmelding | null): void => {
    void emit('meter-einde', { bron, reden, fout } satisfies MeterEinde);
  };

  const stopMeter = (bron: Meetbron): boolean => {
    const meter = lopend.get(bron);
    if (meter === undefined) return false;
    window.clearInterval(meter.timer);
    lopend.delete(bron);
    return true;
  };

  // Een niveau dat op en neer gaat, met stiltes ertussen, zoals iemand die praat.
  const microfoonNiveau = (t: number): number => {
    if (opties.has('meterstil')) return 0.02 + Math.random() * 0.03;
    const golf = (Math.sin(t / 650) + 1) / 2;
    const spreekt = t > 1200 && Math.sin(t / 2400) > -0.3;
    return spreekt ? 0.12 + 0.7 * golf * (0.6 + 0.4 * Math.random()) : 0.03 + Math.random() * 0.04;
  };

  const systeemNiveau = (t: number): number => {
    if (Date.now() < testgeluidTot) return 0.62 + Math.random() * 0.06;
    if (opties.has('metermuziek'))
      return 0.35 + 0.3 * ((Math.sin(t / 400) + 1) / 2) * Math.random();
    // Speelt de computer niets af, dan levert Windows niets: het niveau is 0.
    return 0;
  };

  const startMeter = (bron: Meetbron): void => {
    stopMeter(bron);
    const begin = Date.now();
    const meter: Lopend = {
      begin,
      laatsteHartslag: begin,
      timer: window.setInterval(() => {
        const nu = Date.now();
        if (nu - meter.laatsteHartslag > HARTSLAG_TERMIJN_MS) {
          stopMeter(bron);
          meldEinde(bron, 'hartslag', null);
          return;
        }
        if (bron === 'microfoon' && opties.has('meteruitval') && nu - begin > 4000) {
          stopMeter(bron);
          meldEinde(bron, 'fout', {
            melding:
              'De microfoon is losgekoppeld tijdens het opnemen. Sluit hem opnieuw aan of kies een andere.',
            vervolg: 'microfooninstelling_openen'
          });
          return;
        }
        const t = nu - begin;
        const niveau = bron === 'microfoon' ? microfoonNiveau(t) : systeemNiveau(t);
        void emit('meter-niveau', { bron, niveau } satisfies MeterNiveau);
      }, TIK_MS)
    };
    lopend.set(bron, meter);
  };

  return (cmd, args) => {
    switch (cmd) {
      // Een vergaderopname gaat voor, zoals in Rust: alle meters stoppen. Het commando zelf
      // handelt de vergadermock af.
      case 'vergadering_start':
        for (const bron of ['microfoon', 'systeem'] as const) {
          if (stopMeter(bron)) meldEinde(bron, 'voorrang', null);
        }
        return ONBEKEND;
      case 'meter_start': {
        const bron = bronUit(args);
        if (bron === null)
          throw { melding: 'Onbekende bron.', vervolg: 'geen' } satisfies Foutmelding;
        if (bron === 'microfoon' && opties.has('meterfout')) {
          throw {
            melding:
              'Een ander programma gebruikt de microfoon. Sluit dat programma en probeer het opnieuw.',
            vervolg: 'opnieuw_proberen'
          } satisfies Foutmelding;
        }
        if (bron === 'systeem' && opties.has('metergeenapp')) {
          throw {
            melding:
              'Het geluid van de computer opnemen kan op dit systeem nog niet. Alleen de microfoon wordt opgenomen.',
            vervolg: 'geen'
          } satisfies Foutmelding;
        }
        // De microfoon opent in de mock in een kwart seconde, het systeemgeluid meteen.
        const wacht = bron === 'microfoon' ? 250 : 20;
        return new Promise((klaar) => {
          window.setTimeout(() => {
            startMeter(bron);
            klaar(null);
          }, wacht);
        });
      }
      case 'meter_hartslag': {
        const bron = bronUit(args);
        const meter = bron === null ? undefined : lopend.get(bron);
        if (meter === undefined) return false;
        meter.laatsteHartslag = Date.now();
        return true;
      }
      case 'meter_stop': {
        const bron = bronUit(args);
        if (bron === null) {
          stopMeter('microfoon');
          stopMeter('systeem');
        } else {
          stopMeter(bron);
        }
        return null;
      }
      case 'testgeluid_speel': {
        if (opties.has('testgeluidfout')) {
          throw {
            melding:
              'Mondig kon het testgeluid niet afspelen. Kies een ander apparaat of probeer het opnieuw.',
            vervolg: 'opnieuw_proberen'
          } satisfies Foutmelding;
        }
        if (Date.now() < testgeluidTot) return null;
        testgeluidTot = Date.now() + TESTGELUID_MS;
        return new Promise((klaar) => {
          window.setTimeout(() => {
            klaar(null);
          }, TESTGELUID_MS + 150);
        });
      }
      default:
        return ONBEKEND;
    }
  };
}
