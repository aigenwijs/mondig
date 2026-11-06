// De ene bron van waarheid voor de dictaatgeschiedenis (ADR 0021): de termijn en de bewaarde
// dictaten. Rust is de baas; deze stand haalt ze op en ververst bij het event
// `dictaten-gewijzigd` (nieuw dictaat, verwijderd, verlopen, termijn veranderd). Zo luistert
// niets naar alleen de mock: de lijst volgt altijd wat Rust meldt. De acties hieronder
// verversen daarom zelf niet: elk Rust-commando dat iets wijzigt stuurt dat event.
//
// De stand houdt ook een klok bij die elke minuut tikt, voor Vandaag en Gisteren na middernacht
// en om wat verlopen is weg te laten tussen twee opruimrondes van Rust in.

import {
  dictaatVerwijder,
  dictatenBewarenZet,
  dictatenStand as haalStand,
  dictatenWisAlles
} from './commandos';
import {
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
  /** De bewaarde dictaten, nieuwste eerst. Leeg bij Niet bewaren. */
  dictaten = $state<BewaardDictaat[]>([]);
  /** Het aantal bestanden op schijf, ook wat nog moet verlopen (voor de bevestigvraag). */
  bewaard = $state(0);
  /** Het laatste dictaat in het geheugen van de app. */
  laatste = $state<LaatsteDictaat | null>(null);
  /** De klok (ms sinds 1970), elke minuut bijgewerkt zolang de stand luistert. */
  nu = $state(Date.now());
  /** De lijst zonder wat intussen verlopen is. */
  zichtbaar = $derived(filterVerlopen(this.dictaten, this.termijn, this.nu));
  /** Heeft Rust al een keer geantwoord? Daarvoor tonen we geen lege staat. */
  geladen = $state(false);
  /** Telt de vragen, zodat een late oudere reactie een nieuwere niet overschrijft. */
  private volgnummer = 0;

  /** Haalt de stand opnieuw op. Buiten de app bestaat het commando niet; dan blijft het zoals het was. */
  async ververs(): Promise<void> {
    const nummer = ++this.volgnummer;
    try {
      const antwoord = await haalStand();
      if (nummer !== this.volgnummer) return;
      this.termijn = termijnUit(antwoord.termijn);
      this.dictaten = antwoord.dictaten;
      this.bewaard = antwoord.bewaard;
      this.laatste = antwoord.laatste;
      this.geladen = true;
    } catch {
      // Zonder antwoord blijft de stand zoals hij was.
    }
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
    await dictatenBewarenZet(termijn);
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
