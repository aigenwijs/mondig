// De laatste controle van "Mondig opnieuw afstellen" (gebruikerstest na N-C): Modellen toont er
// "Past bij deze computer" en de datum mee, Instellingen werkt hem bij na een afgeronde ronde.
// Eén stand, zodat beide schermen hetzelfde zeggen.

import {
  laatsteControle,
  laatsteControleBewaar,
  type Advies,
  type LaatsteControle
} from './computer';

class ControleStand {
  /** De laatste controle; `null` als Mondig deze computer nog niet bekeek. */
  controle = $state<LaatsteControle | null>(null);
  /** Is de stand opgehaald? Tot dan geen "Laat Mondig je computer bekijken". */
  geladen = $state(false);
  /** Telt op bij elke laad() en bewaar(); een ouder antwoord overschrijft nooit een nieuwer. */
  private volgnummer = 0;

  async laad(): Promise<void> {
    const mijn = ++this.volgnummer;
    let gelezen: LaatsteControle | null;
    try {
      gelezen = await laatsteControle();
    } catch {
      // Niet te lezen: dan zegt Modellen niets over passen, net als zonder controle.
      gelezen = null;
    }
    // Kwam er intussen een bewaar() (of een nieuwere laad()), dan is dit antwoord verouderd.
    if (mijn !== this.volgnummer) return;
    this.controle = gelezen;
    this.geladen = true;
  }

  /** Bewaart een afgeronde controle; mislukt dat, dan blijft de vorige staan. */
  async bewaar(advies: Advies): Promise<void> {
    const mijn = ++this.volgnummer;
    try {
      const bewaard = await laatsteControleBewaar(advies);
      if (mijn !== this.volgnummer) return;
      this.controle = bewaard;
      this.geladen = true;
    } catch {
      // Het advies zelf staat al in beeld; alleen het label op Modellen blijft zoals het was.
    }
  }
}

export const controleStand = new ControleStand();
export { ControleStand };
