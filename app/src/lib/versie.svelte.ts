// De versiegegevens, één keer opgehaald en gedeeld door de voet van de zijbalk, het scherm
// Over Mondig en de kaart "Wat is er nieuw". De gegevens veranderen niet zolang de app draait.

import { haalVersieInfo, type VersieInfo } from './versie';

class VersieStand {
  info = $state<VersieInfo | null>(null);
  mislukt = $state(false);
  private belofte: Promise<void> | null = null;

  /** Haalt de gegevens op als dat nog niet gebeurd is. Veilig om vaker aan te roepen. */
  laad(): Promise<void> {
    this.belofte ??= haalVersieInfo()
      .then((info) => {
        this.info = info;
      })
      .catch(() => {
        this.mislukt = true;
        this.belofte = null;
      });
    return this.belofte;
  }
}

export const versieStand = new VersieStand();
