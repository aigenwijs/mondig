// De stand van de bewaakte map in Instellingen, gedeeld door de rijen van de groep "Map in de
// gaten houden". Elke wijziging gaat meteen naar Rust; bij een fout blijft de oude stand staan
// en staat de fout onder de rij.

import {
  bewaakInstellingen,
  bewaakKiesMap,
  bewaakZetAan,
  bewaakZetNaast,
  bewaakZetOverslaan,
  bewaakZetSubmappen,
  type BewaakInstellingen
} from './bewaakte-map';
import { foutUitCommando } from './taak';

/** Welke rij het laatst wijzigde: daar komt "Opgeslagen." of de fout. */
export type BewaakRij = 'aan' | 'map' | 'submappen' | 'overslaan' | 'naast';

class BewaakStand {
  stand = $state<BewaakInstellingen | null>(null);
  waar = $state<BewaakRij>('aan');
  bewaard = $state(false);
  fout = $state<string | null>(null);
  kiezen = $state(false);

  /** Haalt de stand op; elke rij die verschijnt mag dit aanroepen. */
  laad(foutTekst: string): void {
    bewaakInstellingen()
      .then((nieuw) => {
        this.stand = nieuw;
      })
      .catch((e: unknown) => {
        this.fout = foutUitCommando(e, foutTekst).melding;
      });
  }

  async wijzig(
    rij: BewaakRij,
    actie: () => Promise<BewaakInstellingen>,
    foutTekst: string
  ): Promise<void> {
    this.waar = rij;
    this.bewaard = false;
    this.fout = null;
    try {
      this.stand = await actie();
      this.bewaard = true;
    } catch (e) {
      this.fout = foutUitCommando(e, foutTekst).melding;
    }
  }

  async kiesMap(foutTekst: string): Promise<void> {
    if (this.kiezen) return;
    this.kiezen = true;
    await this.wijzig('map', bewaakKiesMap, foutTekst);
    this.kiezen = false;
  }

  /**
   * Zet de bewaking aan of uit. Aanzetten zonder map opent eerst de mapkiezer: wie op de
   * schakelaar klikt, wil een map in de gaten houden en hoeft dat niet eerst elders te regelen.
   */
  async zetAan(aan: boolean, foutTekst: string): Promise<void> {
    if (aan && !this.stand?.map) {
      await this.kiesMap(foutTekst);
      if (!this.stand?.map) return;
    }
    await this.wijzig('aan', () => bewaakZetAan(aan), foutTekst);
  }

  zetSubmappen(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('submappen', () => bewaakZetSubmappen(aan), foutTekst);
  }

  zetOverslaan(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('overslaan', () => bewaakZetOverslaan(aan), foutTekst);
  }

  zetNaast(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('naast', () => bewaakZetNaast(aan), foutTekst);
  }
}

export const bewaakStand = new BewaakStand();
