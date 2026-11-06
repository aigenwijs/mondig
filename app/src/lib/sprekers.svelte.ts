// De toestand van "Sprekers herkennen" (4b-D, fase D-2): de ene schakelaar die zowel in
// Instellingen als op Bestanden staat. De beslissingen zelf staan in `sprekers.ts`; dit bestand
// houdt alleen bij wat er nu aan de hand is en praat met Rust en met de modellenstand.

import { sprekersInstelling, sprekersZet } from './instellingen';
import { t } from './i18n/index.svelte';
import { zoekModel, type ModelRegel } from './modellen';
import type { Foutmelding } from './taak';
import { modellenStand } from './modellenstand.svelte';
import {
  SPREKERMODEL,
  bijAanzetten,
  naModelwijziging,
  schakelaarAan,
  weergave,
  type PoortFase,
  type PoortWeergave
} from './sprekers';

/** Waar de poort van afhangt; in tests vervangen door een nep. */
export interface Afhankelijkheden {
  lees: () => Promise<boolean>;
  schrijf: (aan: boolean) => Promise<void>;
  /** Het sprekermodel uit het overzicht van Modellen. */
  model: () => ModelRegel | undefined;
  /** Wacht het sprekermodel in de downloadrij of komt het binnen? */
  actief: () => boolean;
  start: (regel: ModelRegel) => Promise<Foutmelding | null>;
  stop: () => void;
  herlaad: () => Promise<void>;
  /** De melding van de laatste mislukte download van het sprekermodel. */
  downloadFout: () => string | null;
  opslaanFout: () => string;
}

export class Sprekers {
  /** Wat Rust bewaard heeft. Standaard aan, maar zonder model gebeurt er niets. */
  aan = $state(false);
  fase = $state<PoortFase>('rust');
  /** Wat er misging: het bewaren van de instelling of de download. */
  fout = $state<string | null>(null);
  /** Net na een geslaagde download aangezet: de bevestiging staat onder de schakelaar. */
  netAangezet = $state(false);

  // Reactief, zodat een effect dat volg() aanroept er nooit zijn afhankelijkheden door verliest.
  private gestart = $state(false);
  private controleert = false;

  constructor(private readonly deps: Afhankelijkheden) {}

  /** Het sprekermodel uit het overzicht van de pagina Modellen. */
  get model(): ModelRegel | undefined {
    return this.deps.model();
  }

  /** Wacht het sprekermodel in de downloadrij of komt het binnen? */
  get downloadLoopt(): boolean {
    return this.deps.actief();
  }

  /** Staat de schakelaar aan (instelling aan en model aanwezig)? */
  get schakelaar(): boolean {
    return schakelaarAan(this.aan, this.model?.status);
  }

  /** Wat onder de schakelaar staat: niets, de vraag, de stand van de download of de bevestiging. */
  get weergave(): PoortWeergave {
    return weergave(this.fase, this.netAangezet);
  }

  /** Leest de bewaarde instelling. */
  async laad(): Promise<void> {
    try {
      this.aan = await this.deps.lees();
    } catch {
      this.aan = false;
    }
  }

  /** De gebruiker zet de schakelaar om. Aanzetten zonder model vraagt eerst om de download. */
  async zet(nieuw: boolean): Promise<void> {
    this.fout = null;
    this.netAangezet = false;
    if (!nieuw) {
      this.fase = 'rust';
      this.gestart = false;
      await this.bewaar(false);
    } else if (bijAanzetten(this.model?.status) === 'bewaar') {
      await this.bewaar(true);
    } else if (this.downloadLoopt) {
      // Het model komt al binnen (bijvoorbeeld via Modellen gestart): niet nog eens vragen.
      this.fase = 'wacht';
      this.gestart = true;
    } else {
      this.fase = 'vraag';
    }
  }

  /** De gebruiker weigert de download of stopt hem: de schakelaar blijft uit. */
  weiger(): void {
    const loopt = this.downloadLoopt;
    this.fase = 'rust';
    this.gestart = false;
    // Wat al binnen is, blijft bewaard (zoals op de pagina Modellen).
    if (loopt) this.deps.stop();
  }

  /**
   * De gebruiker wil het model downloaden. De schakelaar gaat aan zodra het er is. Meteen na de
   * klik maakt de vraag plaats voor de stand van de download; een tweede klik doet niets.
   */
  async download(): Promise<void> {
    const regel = this.model;
    if (!regel || this.fase === 'wacht') return;
    this.fase = 'wacht';
    if (this.downloadLoopt) {
      // Staat al in de rij: niets dubbel starten, alleen meekijken.
      this.gestart = true;
      return;
    }
    this.gestart = false;
    const fout = await this.deps.start(regel);
    if (fout) {
      this.fase = 'rust';
      this.fout = fout.melding;
    } else if ((this.fase as PoortFase) !== 'wacht') {
      // Geannuleerd (weiger) terwijl de download nog gestart werd: alsnog stoppen.
      this.deps.stop();
    } else {
      this.gestart = true;
    }
  }

  /**
   * Kijkt of de poort verder kan na een verandering van het model of de download. Aanroepen uit
   * een effect. Alles wat reactief is, wordt eerst gelezen, voordat er iets vroegtijdig stopt.
   */
  async volg(): Promise<void> {
    const fase = this.fase;
    const status = this.model?.status;
    const loopt = this.downloadLoopt;
    const gestart = this.gestart;
    if (this.controleert) return;
    let besluit = naModelwijziging(fase, status, loopt, gestart);
    if (besluit === 'opgeven') {
      // Het einde van de download komt eerder binnen dan het nieuwe overzicht: eerst verversen.
      this.controleert = true;
      await this.deps.herlaad();
      this.controleert = false;
      besluit = naModelwijziging(this.fase, this.model?.status, this.downloadLoopt, this.gestart);
    }
    if (besluit === 'aanzetten') {
      this.fase = 'rust';
      this.gestart = false;
      await this.bewaar(true);
      // Zichtbaar: de schakelaar springt aan en er staat een bevestiging onder.
      this.netAangezet = this.aan;
    } else if (besluit === 'opgeven') {
      this.fase = 'rust';
      this.gestart = false;
      this.fout = this.deps.downloadFout();
    }
  }

  private async bewaar(aan: boolean): Promise<void> {
    const vorige = this.aan;
    this.aan = aan;
    try {
      await this.deps.schrijf(aan);
    } catch {
      // De schakelaar laat weer zien wat echt geldt.
      this.aan = vorige;
      this.fout = this.deps.opslaanFout();
    }
  }
}

export const sprekers = new Sprekers({
  lees: async () => (await sprekersInstelling()).aan,
  schrijf: sprekersZet,
  model: () =>
    modellenStand.overzicht ? zoekModel(modellenStand.overzicht.modellen, SPREKERMODEL) : undefined,
  actief: () => modellenStand.isActief(SPREKERMODEL),
  start: (regel) => modellenStand.start(regel, []),
  stop: () => {
    void modellenStand.stop(SPREKERMODEL);
  },
  herlaad: () => modellenStand.laad(),
  downloadFout: () => modellenStand.foutVoor(SPREKERMODEL)?.melding ?? null,
  opslaanFout: () => t().sprekersOpslaanFout
});
