// Het geluidje bij starten en stoppen: de schakelaar staat in de groep Dicteren, het volume
// achter "Meer opties". Beide delen deze stand, anders overschrijft de ene het volume of de
// stand van de andere met een oude waarde.

import { geluidInstelling, geluidZet, type GeluidInstelling } from './instellingen';
import { foutUitCommando } from './taak';

class GeluidStand {
  instelling = $state<GeluidInstelling>({ aan: true, volume: 50 });
  bewaard = $state(false);
  fout = $state<string | null>(null);

  laad(): void {
    geluidInstelling()
      .then((waarde) => {
        this.instelling = waarde;
      })
      .catch(() => undefined);
  }

  /** Bewaart de nieuwe stand; `proef` laat het startgeluid horen. Bij een fout blijft de oude staan. */
  async bewaar(nieuw: GeluidInstelling, proef: boolean, foutTekst: string): Promise<void> {
    const vorige = this.instelling;
    this.instelling = nieuw;
    this.bewaard = false;
    this.fout = null;
    try {
      await geluidZet(nieuw, proef);
      this.bewaard = true;
    } catch (e) {
      this.fout = foutUitCommando(e, foutTekst).melding;
      this.instelling = vorige;
    }
  }
}

export const geluidStand = new GeluidStand();
