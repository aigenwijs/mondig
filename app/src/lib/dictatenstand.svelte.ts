// De ene bron van waarheid voor de dictaatgeschiedenis (ADR 0021): de termijn en de bewaarde
// dictaten. Rust is de baas; deze stand haalt ze op en ververst bij het event
// `dictaten-gewijzigd` (nieuw dictaat, verwijderd, verlopen, termijn veranderd). Zo luistert
// niets naar alleen de mock: de lijst volgt altijd wat Rust meldt. De acties hieronder
// verversen daarom zelf niet: elk Rust-commando dat iets wijzigt stuurt dat event.
//
// De lijst komt in delen (10, dan 20 erbij per klik op "Meer tonen"): Rust stuurt alleen de
// nieuwste `gevraagd` dictaten en het totaal, zodat niet alle tekst over de brug hoeft. Komt er
// een nieuw dictaat bovenaan terwijl er extra geladen is, dan vraagt de stand er evenveel bij,
// zodat de extra dictaten niet verspringen of onderaan verdwijnen.
//
// De stand houdt ook een klok bij die elke minuut tikt, voor Vandaag en Gisteren na middernacht
// en om wat verlopen is weg te laten tussen twee opruimrondes van Rust in.

import {
  dictaatVerwijder,
  dictatenBewarenZet,
  dictatenStand as haalStand,
  dictatenWegval,
  dictatenWisAlles
} from './commandos';
import {
  LIJST_EERSTE,
  LIJST_MEER,
  STANDAARD_DICTATEN_TERMIJN,
  termijnUit,
  filterVerlopen,
  type BewaardDictaat,
  type DictatenTermijn,
  type LaatsteDictaat
} from './dictaten';
import { luisterNaarDictatenGewijzigd } from './gebeurtenissen';

export class DictatenStand {
  /** Hoe lang dictaten bewaard blijven. Tot Rust antwoordt geldt de standaard. */
  termijn = $state<DictatenTermijn>(STANDAARD_DICTATEN_TERMIJN);
  /** De geladen bewaarde dictaten, nieuwste eerst. Leeg bij Niet bewaren. */
  dictaten = $state<BewaardDictaat[]>([]);
  /** Hoeveel dictaten er in totaal gelden (ook wat nog niet geladen is), voor "10 van 37". */
  totaal = $state(0);
  /** Zoveel dictaten vraagt de stand aan Rust: 10, en 20 meer per klik op "Meer tonen". */
  gevraagd = $state(LIJST_EERSTE);
  /** Het aantal bestanden op schijf, ook wat nog moet verlopen (voor de bevestigvraag). */
  bewaard = $state(0);
  /** Het laatste dictaat in het geheugen van de app. */
  laatste = $state<LaatsteDictaat | null>(null);
  /** De klok (ms sinds 1970), elke minuut bijgewerkt zolang de stand luistert. */
  nu = $state(Date.now());
  /** De lijst zonder wat intussen verlopen is. */
  zichtbaar = $derived(filterVerlopen(this.dictaten, this.termijn, this.nu));
  /** Het totaal zonder wat intussen verlopen is: wat de telling "10 van 37" als 37 toont. */
  zichtbaarTotaal = $derived(
    Math.max(this.zichtbaar.length, this.totaal - (this.dictaten.length - this.zichtbaar.length))
  );
  /** Heeft Rust al een keer geantwoord? Daarvoor tonen we geen lege staat. */
  geladen = $state(false);
  /** Telt de vragen, zodat een late oudere reactie een nieuwere niet overschrijft. */
  private volgnummer = 0;

  /** Haalt de stand opnieuw op. Buiten de app bestaat het commando niet; dan blijft het zoals het was. */
  async ververs(): Promise<void> {
    const nummer = ++this.volgnummer;
    try {
      let antwoord = await haalStand(0, this.gevraagd);
      if (nummer !== this.volgnummer) return;
      // Is er extra geladen en staat er nu iets nieuws bovenaan, dan schuift alles een plaats
      // op. Vraag er evenveel bij, zodat het laatste dictaat dat je al zag niet verdwijnt.
      const nieuwBoven = this.nieuwBovenaan(antwoord.dictaten);
      if (nieuwBoven > 0) {
        this.gevraagd += nieuwBoven;
        antwoord = await haalStand(0, this.gevraagd);
        if (nummer !== this.volgnummer) return;
      }
      this.termijn = termijnUit(antwoord.termijn);
      this.dictaten = antwoord.dictaten;
      this.totaal = antwoord.totaal;
      this.bewaard = antwoord.bewaard;
      this.laatste = antwoord.laatste;
      this.geladen = true;
    } catch {
      // Zonder antwoord blijft de stand zoals hij was.
    }
  }

  /**
   * Hoeveel dictaten staan er in dit antwoord boven het nieuwste dat we al hadden? Alleen
   * als de lijst verder is uitgeklapt dan de eerste tien; anders schuift het oudste er gewoon af.
   */
  private nieuwBovenaan(antwoord: readonly BewaardDictaat[]): number {
    if (this.gevraagd <= LIJST_EERSTE || this.dictaten.length === 0) return 0;
    const i = antwoord.findIndex((d) => this.dictaten.some((bekend) => bekend.id === d.id));
    return Math.max(i, 0);
  }

  /** Toont 20 dictaten meer. De lijst volgt na het ophalen. */
  async meerTonen(): Promise<void> {
    this.gevraagd += LIJST_MEER;
    await this.ververs();
  }

  /** Hoeveel dictaten vallen weg als deze termijn gekozen wordt? Voor de bevestigvraag. */
  wegval(termijn: DictatenTermijn): Promise<number> {
    return dictatenWegval(termijn);
  }

  /** Zet de klok op nu. */
  tik(): void {
    this.nu = Date.now();
  }

  /** Verwijdert één dictaat. De lijst volgt via het event. Geeft de fout door. */
  async verwijder(id: string): Promise<void> {
    await dictaatVerwijder(id);
  }

  /** Wist alle bewaarde dictaten. De lijst volgt via het event. */
  async wisAlles(): Promise<void> {
    await dictatenWisAlles();
  }

  /** Zet een andere termijn. De stand volgt via het event. */
  async zetTermijn(termijn: DictatenTermijn): Promise<void> {
    // Een andere stand begint weer met de eerste tien. Dat staat vooraf al goed: het event van
    // Rust kan binnenkomen voor het commando klaar is, en die verversing moet de basislimiet
    // vragen. Mislukt het, dan krijgt de lijst haar uitgeklapte staat terug.
    const eerder = this.gevraagd;
    this.gevraagd = LIJST_EERSTE;
    try {
      await dictatenBewarenZet(termijn);
    } catch (fout) {
      this.gevraagd = eerder;
      throw fout;
    }
  }
}

export const dictatenStand = new DictatenStand();

let luisteraars = 0;
let opruimen: (() => void) | null = null;

/**
 * Begint met luisteren naar `dictaten-gewijzigd` en haalt de stand op. Mag vaker aangeroepen
 * worden (de eerste telt). Geeft een functie die weer stopt.
 */
export function startDictatenStand(): () => void {
  luisteraars += 1;
  if (luisteraars === 1) {
    let weg = false;
    let stop: (() => void) | null = null;
    void luisterNaarDictatenGewijzigd(() => {
      void dictatenStand.ververs();
    }).then((u) => {
      if (weg) u();
      else stop = u;
    });
    void dictatenStand.ververs();
    dictatenStand.tik();
    const klok = setInterval(() => {
      dictatenStand.tik();
    }, 60_000);
    opruimen = () => {
      weg = true;
      stop?.();
      clearInterval(klok);
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
