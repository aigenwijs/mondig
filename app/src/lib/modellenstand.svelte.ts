// De ene bron van waarheid voor de modellen en de downloadrij. De pagina Modellen, de zijbalk
// en de poort bij "Sprekers herkennen" lezen dezelfde stand; de events worden hier één keer
// beluisterd, in de root-layout. Downloads lopen in Rust in een rij (na elkaar, zie
// `downloadrij.ts`), dus navigeren in de app onderbreekt niets.

import { SvelteSet } from 'svelte/reactivity';
import { downloadsLijst, downloadStop } from './commandos';
import {
  isActief,
  isBezig,
  itemVoor,
  knopFase,
  procentVan,
  verdwijntVanzelf,
  zijbalkItems,
  ZIJBALK_NAGLOEI_MS,
  type DownloadItem,
  type KnopFase,
  type RijStand
} from './downloadrij';
import {
  luisterNaarDownloads,
  luisterNaarDownloadVoortgang,
  luisterNaarModellenGewijzigd
} from './gebeurtenissen';
import { t } from './i18n/index.svelte';
import {
  modelDownloadStart,
  modelGebruikZet,
  modelKiezen,
  modellenGezien,
  modellenOverzicht,
  modelVerwijder,
  zoekModel,
  type Gebruik,
  type ModelRegel,
  type ModellenOverzicht
} from './modellen';
import { resterendeTijdTekst, schatResterendeSeconden, type Meting } from './resterendetijd';
import { bereikteDrempel } from './schermlezer';
import { foutUitCommando, type DownloadVoortgang, type Foutmelding } from './taak';

/** De download die nu binnenkomt. */
export interface LopendeModelDownload {
  model: string;
  voortgang: DownloadVoortgang | null;
  /** "Nog ongeveer 2 minuten", of `null` zolang er te kort gemeten is. */
  resterend: string | null;
  /** Voor welke taken het model na de download in gebruik komt (voor zover bekend). */
  daarna: readonly Gebruik[];
}

export class ModellenStand {
  overzicht = $state<ModellenOverzicht | null>(null);
  /** Het overzicht was niet op te halen. */
  laadFout = $state(false);
  /** De downloadrij, zoals Rust hem laatst meldde. */
  rij = $state<DownloadItem[]>([]);
  /** Een zin voor schermlezers bij 25, 50, 75 en 100 procent. */
  schermlezerZin = $state('');
  /** Is er een model dat de gebruiker nog niet zag? Voor de badge in de zijbalk. */
  heeftNieuw = $derived(this.overzicht?.modellen.some((m) => m.nieuw) ?? false);

  /** Het volgnummer van de laatst verwerkte stand van de rij; oudere standen tellen niet. */
  private versie = -1;
  /** Modellen waarvan net op Downloaden geklikt is en Rust nog niet antwoordde. */
  private startend = new SvelteSet<string>();
  /** Items die niet meer in de zijbalk staan (klaar en even later, of gezien). */
  private verborgen = new SvelteSet<number>();
  /** Wat er misging bij het starten (voordat het model in de rij stond). */
  private startFout = $state<{ model: string; fout: Foutmelding } | null>(null);
  /** De resterende tijd per item, als tekst. */
  private resterend = $state<Record<number, string | null>>({});
  /** De taken die bij het starten gekozen werden, per model. */
  private daarna = $state<Record<string, readonly Gebruik[]>>({});
  // Bewust niet reactief: alleen voor de berekening, niet voor het scherm.
  private metingen: Record<number, Meting[] | undefined> = {};
  private vorigProcent: Record<number, number | undefined> = {};
  private wachtOpVerbergen: Record<number, true | undefined> = {};

  /** Het item dat nu binnenkomt. */
  private bezigItem = $derived(this.rij.find((item) => isBezig(item.toestand)));

  /** De download die nu binnenkomt, of `null`. */
  lopend = $derived.by((): LopendeModelDownload | null => {
    const item = this.bezigItem;
    if (!item) return null;
    const regel = this.overzicht ? zoekModel(this.overzicht.modellen, item.model) : undefined;
    return {
      model: item.model,
      voortgang: item.voortgang,
      resterend: this.resterend[item.id] ?? null,
      daarna: this.daarna[item.model] ?? regel?.daarnaBijHervatten ?? []
    };
  });

  /** Het procent van de download die nu binnenkomt, of 0. */
  procent = $derived(procentVan(this.bezigItem?.voortgang));

  /** De regel van het model dat binnenkomt. */
  lopendModel = $derived.by(() => {
    const id = this.bezigItem?.model;
    if (!id || !this.overzicht) return undefined;
    return zoekModel(this.overzicht.modellen, id);
  });

  /** De laatste fout bij een download: bij het starten, of van een item uit de rij. */
  downloadFout = $derived.by((): { model: string; fout: Foutmelding } | null => {
    if (this.startFout) return this.startFout;
    const mislukt = [...this.rij].reverse().find((item) => item.toestand.type === 'fout');
    if (mislukt?.toestand.type !== 'fout') return null;
    return {
      model: mislukt.model,
      fout: { melding: mislukt.toestand.melding, vervolg: mislukt.toestand.vervolg }
    };
  });

  /** Wat de zijbalk toont. */
  zijbalk = $derived(zijbalkItems(this.rij, this.verborgen));

  /** Het item van een model in de rij. */
  itemVoor(model: string): DownloadItem | undefined {
    return itemVoor(this.rij, model);
  }

  /** Wacht dit model in de rij of komt het binnen? */
  isActief(model: string): boolean {
    const item = this.itemVoor(model);
    return item !== undefined && isActief(item.toestand);
  }

  /** Wat de Downloaden-knop van dit model zegt. */
  knopFase(model: string): KnopFase {
    return knopFase(this.startend.has(model), this.itemVoor(model));
  }

  /** De resterende tijd van het item van dit model, of `null`. */
  resterendVoor(model: string): string | null {
    const item = this.itemVoor(model);
    return item ? (this.resterend[item.id] ?? null) : null;
  }

  /** De fout bij dit model, als de laatste poging mislukte. */
  foutVoor(model: string): Foutmelding | null {
    if (this.startFout?.model === model) return this.startFout.fout;
    const toestand = this.itemVoor(model)?.toestand;
    return toestand?.type === 'fout'
      ? { melding: toestand.melding, vervolg: toestand.vervolg }
      : null;
  }

  /** Haalt het overzicht opnieuw op. */
  async laad(): Promise<void> {
    try {
      this.overzicht = await modellenOverzicht();
      this.laadFout = false;
      // Een fout bij een model dat er intussen helemaal staat, is achterhaald.
      const fout = this.startFout;
      if (fout && zoekModel(this.overzicht.modellen, fout.model)?.status === 'compleet') {
        this.startFout = null;
      }
    } catch {
      this.laadFout = this.overzicht === null;
    }
  }

  /** Haalt de rij op, voor wie de app opent terwijl er downloads lopen of onderbroken zijn. */
  async laadRij(): Promise<void> {
    const stand = await downloadsLijst().catch(() => null);
    if (stand) this.opRij(stand);
  }

  /** Oude naam van `laadRij`, voor bestaande aanroepers. */
  async haalLopendeOp(): Promise<void> {
    await this.laadRij();
  }

  /**
   * Zet een model in de downloadrij. Geeft de fout als het niet lukt. Met `daarna` = `null`
   * hervat de download met de taken die bij de eerste start gekozen werden. Een tweede klik
   * terwijl het model start of al in de rij staat, doet niets.
   */
  async start(model: ModelRegel, daarna: readonly Gebruik[] | null): Promise<Foutmelding | null> {
    if (this.startend.has(model.id) || this.isActief(model.id)) return null;
    // Meteen zichtbaar: de knop wordt "Wordt gestart…" voordat Rust iets zegt.
    this.startend.add(model.id);
    this.startFout = null;
    this.schermlezerZin = '';
    this.daarna[model.id] = daarna ?? model.daarnaBijHervatten;
    try {
      await modelDownloadStart(model.id, daarna);
      await this.laadRij();
      return null;
    } catch (e) {
      const fout = foutUitCommando(e, t().somsGingErIetsMis);
      this.startFout = { model: model.id, fout };
      await this.laad();
      return fout;
    } finally {
      this.startend.delete(model.id);
    }
  }

  /** Stopt de download van een model (zonder model: wat nu binnenkomt); wat binnen is, blijft. */
  async stop(model?: string): Promise<void> {
    await downloadStop(model).catch(() => undefined);
  }

  /** Voor welke taken komt dit model na zijn download in gebruik (voor zover bekend)? */
  takenNa(model: string): readonly Gebruik[] {
    const regel = this.overzicht ? zoekModel(this.overzicht.modellen, model) : undefined;
    return this.daarna[model] ?? regel?.daarnaBijHervatten ?? [];
  }

  /** Het model in de downloadrij dat na afloop deze taak overneemt, of `null`. */
  volgtVoor(gebruik: Gebruik): string | null {
    const item = this.rij.find(
      (i) => isActief(i.toestand) && this.takenNa(i.model).includes(gebruik)
    );
    return item?.model ?? null;
  }

  /**
   * Kiest een model voor een taak (Wijzigen op Modellen): staat het er, dan meteen; komt het al
   * binnen, dan komt de taak erbij; anders downloaden en daarna gebruiken. Een andere download
   * die deze taak zou overnemen, verliest hem: de nieuwste keuze wint (zoals Rust).
   */
  async kies(gebruik: Gebruik, regel: ModelRegel): Promise<Foutmelding | null> {
    try {
      await modelKiezen(gebruik, regel.id);
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis);
    }
    const ids = [
      ...Object.keys(this.daarna),
      ...(this.overzicht?.modellen.map((m) => m.id) ?? [])
    ].filter((id, i, alle) => alle.indexOf(id) === i);
    for (const id of ids) {
      if (id !== regel.id && this.takenNa(id).includes(gebruik)) {
        this.daarna[id] = this.takenNa(id).filter((g) => g !== gebruik);
      }
    }
    if (regel.status !== 'compleet' && !this.takenNa(regel.id).includes(gebruik)) {
      this.daarna[regel.id] = [...this.takenNa(regel.id), gebruik];
    }
    await this.laad();
    await this.laadRij();
    return null;
  }

  /** Zet een model dat er al staat in gebruik voor een taak. */
  async gebruik(gebruik: Gebruik, model: string): Promise<Foutmelding | null> {
    try {
      await modelGebruikZet(gebruik, model);
      await this.laad();
      return null;
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis);
    }
  }

  /** Haalt een model van de computer. */
  async verwijder(model: string): Promise<Foutmelding | null> {
    try {
      await modelVerwijder(model);
      await this.laad();
      return null;
    } catch (e) {
      return foutUitCommando(e, t().somsGingErIetsMis);
    }
  }

  /** De pagina is geopend: de badge "Nieuw" mag uit de zijbalk. */
  async markeerGezien(): Promise<void> {
    await modellenGezien().catch(() => undefined);
  }

  /**
   * De pagina Modellen is in beeld: fouten en onderbroken downloads zijn gezien en verdwijnen
   * uit de zijbalk. Op de pagina zelf blijven ze bij het model staan.
   */
  markeerBekeken(): void {
    for (const item of this.rij) {
      if (!isActief(item.toestand) && !verdwijntVanzelf(item.toestand)) {
        this.verborgen.add(item.id);
      }
    }
  }

  /**
   * Een nieuwe stand van de rij uit Rust. Een stand met een lager volgnummer dan de vorige is
   * achterhaald (bijvoorbeeld een antwoord op `downloads_lijst` dat pas na een event binnenkomt)
   * en telt niet: anders zou een mislukte download weer "bezig" lijken.
   */
  opRij(stand: RijStand): void {
    if (stand.versie < this.versie) return;
    this.versie = stand.versie;
    const rij = stand.items;
    const vorige = this.rij;
    const nieuw = rij.map((item) => {
      const was = vorige.find((v) => v.id === item.id);
      // De voortgang komt in een eigen event; een verdere stand die we al hadden, houden we.
      const voortgang =
        isBezig(item.toestand) &&
        was?.voortgang &&
        (!item.voortgang || was.voortgang.binnen > item.voortgang.binnen)
          ? was.voortgang
          : item.voortgang;
      if (verdwijntVanzelf(item.toestand) && !this.wachtOpVerbergen[item.id]) {
        this.wachtOpVerbergen[item.id] = true;
        if (item.toestand.type === 'klaar' && was && isActief(was.toestand)) {
          this.schermlezerZin = t().procentGedownloadControleren(100);
        }
        const id = item.id;
        setTimeout(() => {
          this.verborgen.add(id);
        }, ZIJBALK_NAGLOEI_MS);
      }
      if (!isActief(item.toestand)) {
        this.metingen[item.id] = undefined;
        this.vorigProcent[item.id] = undefined;
      }
      return { ...item, voortgang };
    });
    // Een nieuwe start van een model wist de oude fout van dat model.
    const startFout = this.startFout;
    if (startFout && nieuw.some((i) => i.model === startFout.model && isActief(i.toestand))) {
      this.startFout = null;
    }
    this.rij = nieuw;
  }

  /** Byte-voortgang van het item dat binnenkomt. */
  opVoortgang(nieuw: DownloadVoortgang): void {
    if (nieuw.totaal <= 0) return;
    const item = nieuw.id !== undefined ? this.rij.find((i) => i.id === nieuw.id) : this.bezigItem;
    if (!item) return;
    const voortgang = { binnen: nieuw.binnen, totaal: nieuw.totaal };
    this.rij = this.rij.map((i) =>
      i.id === item.id
        ? {
            ...i,
            voortgang,
            // Na een nieuwe poging komen er weer bytes binnen: weer gewoon bezig.
            toestand: i.toestand.type === 'nieuwe_poging' ? { type: 'bezig' } : i.toestand
          }
        : i
    );
    const metingen = this.metingen[item.id] ?? [];
    metingen.push({ tijd: performance.now(), binnen: nieuw.binnen });
    if (metingen.length > 80) metingen.shift();
    this.metingen[item.id] = metingen;
    const seconden = schatResterendeSeconden(metingen, nieuw.totaal);
    this.resterend = {
      ...this.resterend,
      [item.id]: seconden === null ? null : resterendeTijdTekst(seconden, t())
    };
    const nu = procentVan(voortgang);
    const drempel = bereikteDrempel(this.vorigProcent[item.id] ?? nu, nu);
    this.vorigProcent[item.id] = nu;
    if (drempel !== null) {
      this.schermlezerZin =
        drempel === 100
          ? t().procentGedownloadControleren(drempel)
          : t().procentGedownload(drempel);
    }
  }
}

export const modellenStand = new ModellenStand();

let luisteraars = 0;
let opruimen: (() => void) | null = null;

/**
 * Begint met luisteren naar de download- en modelevents. Mag vaker aangeroepen worden (de
 * eerste telt, de rest deelt dezelfde luisteraars). Geeft een functie die weer stopt.
 */
export function startModellenStand(): () => void {
  luisteraars += 1;
  if (luisteraars === 1) {
    let weg = false;
    const opruimers: (() => void)[] = [];
    const houdVast = (stop: () => void): void => {
      if (weg) stop();
      else opruimers.push(stop);
    };
    // Eerst luisteren, dan ophalen: zo gaat er niets verloren tussen die twee.
    void luisterNaarDownloads((stand) => {
      modellenStand.opRij(stand);
    }).then(houdVast);
    void luisterNaarDownloadVoortgang((v) => {
      modellenStand.opVoortgang(v);
    }).then(houdVast);
    void luisterNaarModellenGewijzigd(() => {
      void modellenStand.laad();
    }).then(houdVast);
    void modellenStand.laad();
    void modellenStand.laadRij();
    opruimen = () => {
      weg = true;
      opruimers.forEach((stop) => {
        stop();
      });
    };
  }
  return () => {
    luisteraars -= 1;
    if (luisteraars === 0) {
      opruimen?.();
      opruimen = null;
    }
  };
}
