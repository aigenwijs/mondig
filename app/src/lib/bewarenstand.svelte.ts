// De bewaarinstellingen (map, naam, formaat, tijden, automatisch, audio) delen één stand tussen de
// rijen onder "Opslaan" en de rijen achter "Meer opties": het voorbeeld van de bestandsnaam
// hangt van meerdere keuzes af, dus een wijziging in de ene rij moet in de andere zichtbaar zijn.

import {
  bewarenInstellingen,
  bewarenKiesMap,
  bewarenZetAudio,
  bewarenZetAutomatisch,
  bewarenZetBouwstenen,
  bewarenZetFormaat,
  bewarenZetTijden,
  type BewaarInstellingen,
  type Bouwsteen,
  type Formaat
} from './bewaren';
import { foutUitCommando } from './taak';

/** Welke rij het laatst wijzigde: daar komt "Opgeslagen." of de fout. */
export type BewaarRij = 'map' | 'naam' | 'formaat' | 'tijden' | 'automatisch' | 'audio';

class BewaarStand {
  stand = $state<BewaarInstellingen | null>(null);
  waar = $state<BewaarRij>('map');
  bewaard = $state(false);
  fout = $state<string | null>(null);
  kiezen = $state(false);

  /** Haalt de stand op; elke rij die verschijnt mag dit aanroepen. */
  laad(foutTekst: string): void {
    bewarenInstellingen()
      .then((nieuw) => {
        this.stand = nieuw;
      })
      .catch((e: unknown) => {
        this.fout = foutUitCommando(e, foutTekst).melding;
      });
  }

  /** Voert een wijziging uit en neemt de nieuwe stand over; bij een fout blijft de oude staan. */
  async wijzig(
    rij: BewaarRij,
    actie: () => Promise<BewaarInstellingen>,
    foutTekst: string,
    // Een voorlopige stand die meteen getoond wordt, zodat sleep en toetsen niet achterlopen.
    voorlopig?: BewaarInstellingen
  ): Promise<void> {
    const vorige = this.stand;
    this.waar = rij;
    this.bewaard = false;
    this.fout = null;
    if (voorlopig) this.stand = voorlopig;
    try {
      this.stand = await actie();
      this.bewaard = true;
    } catch (e) {
      this.stand = vorige;
      this.fout = foutUitCommando(e, foutTekst).melding;
    }
  }

  async kiesMap(foutTekst: string): Promise<void> {
    if (this.kiezen) return;
    this.kiezen = true;
    await this.wijzig('map', bewarenKiesMap, foutTekst);
    this.kiezen = false;
  }

  zetFormaat(formaat: Formaat, foutTekst: string): Promise<void> {
    return this.wijzig('formaat', () => bewarenZetFormaat(formaat), foutTekst);
  }

  zetTijden(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('tijden', () => bewarenZetTijden(aan), foutTekst);
  }

  zetAutomatisch(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('automatisch', () => bewarenZetAutomatisch(aan), foutTekst);
  }

  zetAudio(aan: boolean, foutTekst: string): Promise<void> {
    return this.wijzig('audio', () => bewarenZetAudio(aan), foutTekst);
  }

  /** Bewaart de bouwstenen in deze volgorde; de nieuwe volgorde is meteen te zien. */
  zetBouwstenen(bouwstenen: Bouwsteen[], foutTekst: string): Promise<void> {
    const voorlopig = this.stand ? { ...this.stand, bouwstenen } : undefined;
    return this.wijzig('naam', () => bewarenZetBouwstenen(bouwstenen), foutTekst, voorlopig);
  }
}

export const bewaarStand = new BewaarStand();
