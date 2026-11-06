import type { BestandInfo } from './bestanden';

// De geschatte resttijd van de hele wachtrij bij Bestanden (fase B-2, naar de batch-banner van
// Vibe). Het uitschrijven van één bestand heeft al zijn eigen schatting; dit is de som over
// alle bestanden die nog moeten, op basis van de snelheid tot nu toe: hoeveel seconden opname
// Mondig per seconde rekenen uitschrijft, bij de bestanden die klaar zijn en bij het bestand
// dat nu loopt.

/** Zo lang moet er gemeten zijn voordat de schatting verschijnt; eerder springt hij te veel. */
const MIN_MEETTIJD_MS = 5_000;

/** Wat de wachtrij nog te doen heeft. */
export interface Wachtrijschatting {
  /** Geschatte seconden tot alles klaar is. */
  seconden: number;
  /** Hoeveel bestanden nog moeten, het bestand dat nu loopt meegeteld. */
  aantal: number;
}

/** Wat de pagina van een lopend bestand heeft gemeten: sinds wanneer en vanaf welk punt. */
export interface Lopend {
  /** Hoe lang de pagina dit bestand al volgt. */
  ms: number;
  /** De voortgang (procent) bij de eerste meting; wat daarvoor gebeurde, weet de pagina niet. */
  vanafProcent: number;
}

/**
 * Schat hoe lang de wachtrij nog duurt. `lopend` geeft per lopend bestand wat de pagina
 * ervan heeft gemeten (de klok van de webview). Zo telt een pagina die pas openging toen het
 * bestand al halverwege was, alleen de voortgang sinds die tijd mee, anders lijkt het uitschrijven
 * te snel.
 *
 * Geeft `null` bij minder dan twee bestanden (het bestand heeft dan zijn eigen schatting),
 * zolang er te kort gemeten is, of als er nog niets uitgeschreven is waar de snelheid uit te
 * halen valt. Een wachtend bestand zonder bekende duur telt niet mee in de tijd.
 */
export function schatWachtrij(
  lijst: readonly BestandInfo[],
  lopend: (taak: number) => Lopend | undefined
): Wachtrijschatting | null {
  let aantal = 0;
  // Opname die al klaar is of loopt, en de tijd die dat kostte.
  let gedaanAudioMs = 0;
  let gedaanTijdMs = 0;
  // Opname die nog moet.
  let restAudioMs = 0;

  for (const bestand of lijst) {
    const { toestand, duurMs } = bestand;
    const duur = duurMs !== null && duurMs > 0 ? duurMs : 0;
    switch (toestand.type) {
      case 'klaar':
        if (duur > 0 && bestand.verwerkingMs !== null && bestand.verwerkingMs > 0) {
          gedaanAudioMs += duur;
          gedaanTijdMs += bestand.verwerkingMs;
        }
        break;
      case 'wacht':
      case 'luistert':
        aantal += 1;
        restAudioMs += duur;
        break;
      case 'verwerkt': {
        aantal += 1;
        // Het sprekermodel meldt geen voortgang: het uitschrijven is dan klaar.
        const voortgang =
          toestand.stap === 'sprekers' ? 100 : Math.min(100, Math.max(0, toestand.voortgang ?? 0));
        if (duur > 0) {
          const gemeten = lopend(bestand.taak);
          if (gemeten) {
            gedaanAudioMs += (duur * Math.max(0, voortgang - gemeten.vanafProcent)) / 100;
            gedaanTijdMs += gemeten.ms;
          }
          restAudioMs += (duur * (100 - voortgang)) / 100;
        }
        break;
      }
      case 'fout':
      case 'geannuleerd':
        break;
    }
  }

  if (aantal < 2) return null;
  if (gedaanTijdMs < MIN_MEETTIJD_MS || gedaanAudioMs <= 0) return null;
  const audioPerMs = gedaanAudioMs / gedaanTijdMs;
  return { seconden: restAudioMs / audioPerMs / 1000, aantal };
}
