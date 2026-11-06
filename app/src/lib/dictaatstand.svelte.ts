// De ene bron van waarheid voor het lopende dictaat. Het startscherm (groot paneel) en elk
// ander scherm (smalle strook) lezen dezelfde stand; de events worden hier één keer
// beluisterd, in de root-layout. Zo kunnen paneel en strook elkaar nooit tegenspreken en
// is er geen tweede luisteraar die een gebeurtenis dubbel verwerkt.

import { laatsteDictaat } from './commandos';
import {
  RUST,
  opGebeurtenis,
  opUitslag,
  plakHoortBij,
  type PlakUitkomst,
  type PlakUitslag,
  type VorigKlembord,
  type Weergave
} from './dictaat';
import {
  luisterNaarDictaatResultaat,
  luisterNaarPlakUitslag,
  luisterNaarTaken
} from './gebeurtenissen';
import type { Gebeurtenis, ProefResultaat } from './taak';

class DictaatStand {
  /** Waar het huidige dictaat is. */
  weergave = $state<Weergave>(RUST);
  /** De tekst van het laatste dictaat. Alleen in het geheugen, tot het volgende of afsluiten. */
  dictaat = $state<string | null>(null);
  /** Het moment (ms sinds 1970) waarop die tekst klaar was, voor "Laatste dictaat · 14:02". */
  dictaatOm = $state<number | null>(null);
  /** Hoe het plakken van het dictaat in beeld afliep, of `null` zolang dat niet bekend is. */
  plak = $state<PlakUitkomst | null>(null);
  /** Kon het klembord van voor het plakken niet bewaard of teruggezet worden? */
  vorigKlembord = $state<VorigKlembord>('bewaard');
  /**
   * Heeft de gebruiker het startscherm gezien sinds de laatste wijziging? Daar staan de
   * tekst en de vervolgactie, dus de strook op andere schermen hoeft daarna niet meer.
   */
  gezien = $state(true);
  /** Staat het startscherm (met het grote paneel) nu open? */
  private opStartscherm = false;

  opTaak(gebeurtenis: Gebeurtenis): void {
    const vorige = this.weergave;
    this.weergave = opGebeurtenis(vorige, gebeurtenis, Date.now());
    if (this.weergave !== vorige) {
      this.gezien = this.opStartscherm;
      if (this.weergave.type === 'luistert') {
        this.plak = null;
        this.vorigKlembord = 'bewaard';
      }
    }
  }

  opPlak(uitslag: PlakUitslag): void {
    if (plakHoortBij(this.weergave, uitslag)) {
      this.plak = uitslag.uitkomst;
      this.vorigKlembord = uitslag.vorigKlembord;
    }
  }

  opResultaat(uitslag: ProefResultaat): void {
    const vorige = this.weergave;
    this.weergave = opUitslag(vorige, uitslag);
    if (this.weergave !== vorige) {
      if (uitslag.tekst !== null) {
        this.dictaat = uitslag.tekst;
        this.dictaatOm = Date.now();
      }
      this.gezien = this.opStartscherm;
    }
  }

  /** Het startscherm opent of sluit. Bij openen is alles wat er is gezien. */
  zetStartscherm(open: boolean): void {
    this.opStartscherm = open;
    if (open) this.gezien = true;
  }
}

export const dictaatStand = new DictaatStand();

let luisteraars = 0;
let opruimen: (() => void) | null = null;

/**
 * Begint met luisteren naar de dictaat-events. Mag vaker aangeroepen worden (de eerste
 * telt, de rest deelt dezelfde luisteraars). Geeft een functie die weer stopt.
 */
export function startDictaatStand(): () => void {
  luisteraars += 1;
  if (luisteraars === 1) {
    let weg = false;
    const opruimers: (() => void)[] = [];
    const houdVast = (stop: () => void): void => {
      if (weg) stop();
      else opruimers.push(stop);
    };
    void luisterNaarTaken((g) => {
      dictaatStand.opTaak(g);
    }).then(houdVast);
    void luisterNaarDictaatResultaat((u) => {
      dictaatStand.opResultaat(u);
    }).then(houdVast);
    void luisterNaarPlakUitslag((u) => {
      dictaatStand.opPlak(u);
    }).then(houdVast);
    // Buiten de app bestaat het commando niet; dan is er ook geen dictaat.
    laatsteDictaat()
      .then((tekst) => {
        dictaatStand.dictaat ??= tekst;
      })
      .catch(() => undefined);
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
