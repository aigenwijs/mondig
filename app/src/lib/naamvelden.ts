// De kaart "Naam van het bestand" (UI-E): de logica achter de velden Datum, Klant en Onderwerp,
// los van het scherm zodat ze te testen is. Twee dingen:
//
// - het voorbeeld ("Wordt 2026-11-02 Gemeente Vlaardingen Intake.docx") volgt elke invoer, met
//   een korte vertraging zodat Rust niet bij elke toets wordt gevraagd; een antwoord dat te laat
//   komt (er is intussen verder getypt) wordt genegeerd;
// - de velden worden bewaard bij blur, Enter of een keuze uit de suggesties, niet bij elke
//   toets: bij een verslag schrijft bewaren naar schijf.

/** De drie velden zoals de gebruiker ze ziet. */
export interface Naamvelden {
  klant: string;
  onderwerp: string;
  datum: string;
}

/** Hoe lang het voorbeeld wacht op nieuwe invoer voor het Rust vraagt. */
export const VOORBEELD_WACHT_MS = 150;

/** Zijn twee sets velden gelijk? */
export function gelijkeNaamvelden(a: Naamvelden, b: Naamvelden): boolean {
  return a.klant === b.klant && a.onderwerp === b.onderwerp && a.datum === b.datum;
}

/** Waar de kaart mee werkt; het scherm geeft de commando's en de plekken voor de uitkomst. */
export interface NaamveldenWerk {
  /** De bestandsnaam bij deze velden, uit Rust. */
  haalVoorbeeld: (velden: Naamvelden) => Promise<string>;
  /** Bewaart de velden. Geeft de melding bij een fout, anders `null`. */
  bewaar: (velden: Naamvelden) => Promise<string | null>;
  /** Toont het voorbeeld. */
  zetVoorbeeld: (naam: string) => void;
  /** Toont of wist de melding bij een fout. */
  zetFout: (melding: string | null) => void;
  /** Vertraging van het voorbeeld; standaard [`VOORBEELD_WACHT_MS`]. */
  wacht?: number;
}

/** De bediening van de kaart. */
export interface NaamveldenBeheer {
  /** Er is iets getypt of gekozen: het voorbeeld volgt, na een korte vertraging. */
  invoer: (velden: Naamvelden) => void;
  /**
   * Bewaart de velden als ze anders zijn dan wat al bewaard is (bij blur, Enter of een keuze, en
   * vlak voor Opslaan). Geeft `false` als bewaren mislukte.
   */
  bewaarNu: () => Promise<boolean>;
  /** Er kwamen nieuwe bewaarde waarden van buiten (een nieuwe stand): dit is nu "bewaard". */
  bewaard: (velden: Naamvelden) => void;
  /** Ruimt de wachtende vertraging op. */
  stop: () => void;
}

/** Maakt de bediening voor een kaart die met `start` begint (de bewaarde waarden). */
export function maakNaamveldenBeheer(werk: NaamveldenWerk, start: Naamvelden): NaamveldenBeheer {
  const wacht = werk.wacht ?? VOORBEELD_WACHT_MS;
  let huidig = { ...start };
  let opSchijf = { ...start };
  let timer: ReturnType<typeof setTimeout> | null = null;
  // Elke vraag krijgt een nummer; alleen het antwoord op de laatste telt.
  let vraagNummer = 0;
  // De bewaarbeurt die nu loopt, zodat een tweede vraag erop wacht.
  let lopend: Promise<undefined> | null = null;

  function vraagVoorbeeld(velden: Naamvelden): void {
    vraagNummer += 1;
    const nummer = vraagNummer;
    void werk
      .haalVoorbeeld(velden)
      .then((naam) => {
        if (nummer === vraagNummer) werk.zetVoorbeeld(naam);
      })
      .catch(() => {
        // Het voorbeeld is een hulp: lukt het niet, dan blijft het vorige staan.
      });
  }

  function wis(): void {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  return {
    invoer(velden) {
      huidig = { ...velden };
      wis();
      timer = setTimeout(() => {
        timer = null;
        vraagVoorbeeld(huidig);
      }, wacht);
    },
    async bewaarNu() {
      // Loopt er al een bewaarbeurt (blur en dan meteen Opslaan), wacht erop in plaats van een
      // tweede schrijfactie te starten; is er daarna niets nieuws, dan is er niets te bewaren.
      while (lopend !== null) await lopend;
      if (gelijkeNaamvelden(huidig, opSchijf)) return true;
      const teBewaren = { ...huidig };
      const beurt = werk.bewaar(teBewaren).then((melding) => {
        if (melding !== null) {
          werk.zetFout(melding);
          return false;
        }
        opSchijf = teBewaren;
        werk.zetFout(null);
        return true;
      });
      lopend = beurt.then(
        () => undefined,
        () => undefined
      );
      const klaar = lopend;
      try {
        return await beurt;
      } finally {
        await klaar;
        if (lopend === klaar) lopend = null;
      }
    },
    bewaard(velden) {
      // Wat de gebruiker typte en nog niet bewaarde, blijft staan: alleen als er niets is
      // aangepast sinds het laatst bewaarde, is dit ook wat er nu staat.
      const onaangeraakt = gelijkeNaamvelden(huidig, opSchijf);
      opSchijf = { ...velden };
      if (onaangeraakt) huidig = { ...velden };
      wis();
      vraagVoorbeeld(huidig);
    },
    stop: wis
  };
}
