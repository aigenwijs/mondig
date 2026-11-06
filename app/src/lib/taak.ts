// Typen voor het toestandsmodel van mondig-core (crates/mondig-core/src/taak.rs).
// De vorm volgt de serde-uitvoer van Rust: getagde enum (`type`), snake_case-namen.

/** Wat voor taak het is. */
export type Soort = 'dictaat' | 'bestand' | 'vergadering' | 'download';

/** Wat de gebruiker na een fout kan doen. Er is er altijd hooguit één. */
export type Vervolg =
  | 'opnieuw_proberen'
  | 'ander_model_kiezen'
  | 'microfooninstelling_openen'
  | 'opnieuw_downloaden'
  | 'opnieuw_installeren'
  | 'geen';

/** In welke toestand een taak zit. */
/** Welke stap van een taak loopt; een vergadering of bestand kent `sprekers` (4b-D). */
export type Stap = 'uitschrijven' | 'sprekers';

export type Toestand =
  | { type: 'wacht' }
  | { type: 'luistert' }
  | {
      type: 'verwerkt';
      voortgang: number | null;
      /** Uitschrijven, of daarna sprekers herkennen (4b-D). */
      stap?: Stap;
    }
  | { type: 'klaar' }
  | { type: 'fout'; melding: string; vervolg: Vervolg }
  | { type: 'geannuleerd' };

/** Eén wijziging van één taak: de inhoud van het Tauri-event `taak`. */
export interface Gebeurtenis {
  taak: number;
  soort: Soort;
  toestand: Toestand;
}

/** Een fout in gewone taal met de ene vervolgactie, zoals Rust die teruggeeft. */
export interface Foutmelding {
  melding: string;
  vervolg: Vervolg;
}

/**
 * De tekst die de gebruiker bij een fout ziet. Rust geeft meldingen in het Nederlands;
 * voor "Mondig is niet volledig bijgewerkt" (`opnieuw_installeren`) gebruiken we de tekst
 * in de gekozen taal, die de aanroeper meegeeft.
 */
export function meldingTekst(
  fout: { melding: string; vervolg: Vervolg },
  nietBijgewerkt: string
): string {
  return fout.vervolg === 'opnieuw_installeren' ? nietBijgewerkt : fout.melding;
}

/** Inhoud van het event `download-voortgang`, in bytes, met het item uit de downloadrij. */
export interface DownloadVoortgang {
  binnen: number;
  totaal: number;
  /** Het nummer van het item in de downloadrij (alleen in het event). */
  id?: number;
  /** Het model dat binnenkomt (alleen in het event). */
  model?: string;
}

/** Wat het filter van een proefuitslag vindt. */
export type Uitkomst = 'tekst' | 'geen_geluid' | 'geen_tekst' | 'verdacht';

/** Inhoud van het event `proef-resultaat`. */
export interface ProefResultaat {
  taak: number;
  uitkomst: Uitkomst;
  /** Alleen bij uitkomst `tekst`. */
  tekst: string | null;
  duurMs: number;
  soevereiniteit: 'lokaal' | 'europees' | 'buiten_europa';
}

const VERVOLGEN: readonly string[] = [
  'opnieuw_proberen',
  'ander_model_kiezen',
  'microfooninstelling_openen',
  'opnieuw_downloaden',
  'opnieuw_installeren',
  'geen'
];

function isFoutmelding(waarde: unknown): waarde is Foutmelding {
  if (typeof waarde !== 'object' || waarde === null) return false;
  const kandidaat = waarde as Record<string, unknown>;
  return (
    typeof kandidaat['melding'] === 'string' &&
    typeof kandidaat['vervolg'] === 'string' &&
    VERVOLGEN.includes(kandidaat['vervolg'])
  );
}

/**
 * Zet wat een Tauri-commando weigerde om in een `Foutmelding`. Rust geeft een
 * `Foutmelding` terug; alles anders (een tekst, een onverwachte fout) wordt de
 * terugvaltekst zonder vervolg, zodat er nooit een lege of technische melding verschijnt.
 */
export function foutUitCommando(fout: unknown, terugval: string): Foutmelding {
  if (isFoutmelding(fout)) return fout;
  return { melding: terugval, vervolg: 'geen' };
}
