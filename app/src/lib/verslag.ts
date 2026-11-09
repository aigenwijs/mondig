import { invoke } from '@tauri-apps/api/core';

// Het verslag verbeteren, beluisteren en doorzoeken (stroom V, V-1): de pure hulpjes die
// de vergadering (routes/vergadering) en het transcript bij Bestanden (Transcript.svelte)
// delen, via VerslagLijst.svelte. Patroon van Vibe (MIT): regel bewerken (transcript-view.tsx),
// zoeken (transcript-toolbar.tsx) en de snelheden 1, 1,5 en 2 keer (playback-rate.ts).

/** Eén regel van het verslag zoals de lijst hem toont. */
export interface VerslagRegel {
  /** De plek in het verslag: zo heet de regel in de commando's. Uniek binnen de lijst. */
  sleutel: number;
  startMs: number;
  eindMs: number;
  tekst: string;
  /** De persoonsgegevens in de tekst (V-2), in UTF-16-eenheden. */
  markeringen?: Markering[];
  /** Veranderd bij het laatste opnieuw uitschrijven (N-C4): de regel wordt even gemarkeerd. */
  veranderd?: boolean;
  /** De naam van de spreker zoals het label hem toont; zoeken vindt de regel ook daarop. */
  naam?: string | undefined;
}

/** De soorten persoonsgegevens die Mondig markeert; `handmatig` is wat de gebruiker zelf markeerde. */
export type Soort =
  'bsn' | 'iban' | 'telefoon' | 'email' | 'adres' | 'datum' | 'kenteken' | 'handmatig';

/** Een markering van een persoonsgegeven in een regel: plekken in UTF-16-eenheden. */
export interface Markering {
  soort: Soort;
  van: number;
  tot: number;
}

/** De stand van "Persoonsgegevens markeren en vervangen" voor een verslag of transcript. */
export interface PersoonStand {
  /** Wordt er bij Kopiëren en opslaan vervangen? */
  vervangen: boolean;
  /** De stukken die de gebruiker zelf markeerde. */
  eigen: string[];
}

/** Een uniek gemarkeerd stuk voor de lijst op het scherm: de tekst, de soort en hoe vaak. */
export interface Gemarkeerd {
  tekst: string;
  soort: Soort;
  aantal: number;
}

/**
 * De gemarkeerde stukken van het hele verslag, zonder dubbelen (zonder op hoofdletters te letten),
 * op volgorde van eerste voorkomen. Voor de lijst waarin de gebruiker een markering weghaalt.
 */
export function gemarkeerdeStukken(regels: readonly VerslagRegel[]): Gemarkeerd[] {
  const lijst = new Map<string, Gemarkeerd>();
  for (const regel of regels) {
    for (const m of regel.markeringen ?? []) {
      const tekst = regel.tekst.slice(m.van, m.tot);
      const sleutel = tekst.toLowerCase().replace(/\s+/g, ' ');
      const bestaand = lijst.get(sleutel);
      if (bestaand) bestaand.aantal += 1;
      else lijst.set(sleutel, { tekst, soort: m.soort, aantal: 1 });
    }
  }
  return [...lijst.values()];
}

/**
 * Wat er bij vervangen voor in de plaats komt. Gelijk aan `Soort::plaatsvervanger` in Rust
 * (mondig-core, persoonsgegevens.rs): het verslag toont zo wat de ander op het klembord krijgt.
 */
export const PLAATSVERVANGER: Record<Soort, string> = {
  bsn: '[BSN]',
  iban: '[IBAN]',
  telefoon: '[TELEFOON]',
  email: '[E-MAIL]',
  adres: '[ADRES]',
  datum: '[DATUM]',
  kenteken: '[KENTEKEN]',
  handmatig: '[GEGEVEN]'
};

/**
 * De regel zoals hij er na Kopiëren en Opslaan uitziet: elk gemarkeerd stuk is vervangen door
 * zijn plaatsvervanger, en de markeringen wijzen naar de nieuwe plekken. Alleen voor de weergave;
 * verbeteren werkt altijd op de echte tekst.
 */
export function vervangenRegel(regel: VerslagRegel): VerslagRegel {
  const markeringen = [...(regel.markeringen ?? [])].sort((a, b) => a.van - b.van);
  if (markeringen.length === 0) return regel;
  let tekst = '';
  let plek = 0;
  const nieuw: Markering[] = [];
  for (const m of markeringen) {
    // Een markering die over de vorige heen ligt, slaan we over: één plaatsvervanger per stuk.
    // Bekend randgeval: een markering die over twee stukken (twee markeringen) loopt, geeft
    // hier in beeld twee plaatsvervangers, terwijl het klembord er één krijgt.
    if (m.van < plek) continue;
    tekst += regel.tekst.slice(plek, m.van);
    const van = tekst.length;
    tekst += PLAATSVERVANGER[m.soort];
    nieuw.push({ soort: m.soort, van, tot: tekst.length });
    plek = m.tot;
  }
  tekst += regel.tekst.slice(plek);
  return { ...regel, tekst, markeringen: nieuw };
}

// ---------------------------------------------------------------------------
// Zoeken
// ---------------------------------------------------------------------------

/** Een treffer: de regel (plek in de lijst, niet de sleutel) en het stuk tekst dat past. */
export interface Treffer {
  regel: number;
  van: number;
  tot: number;
  /** Een treffer in de naam van de spreker, niet in de tekst (`van` en `tot` zijn dan 0). */
  naam?: true;
}

function ontsnap(tekst: string): string {
  return tekst.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Alle treffers van `zoek` in de teksten, hoofdletterongevoelig, in volgorde van het verslag.
 * Leeg zoeken geeft geen treffers. Treffers in één regel overlappen niet. Met `namen` (per regel
 * de naam van de spreker) telt een naam die het gezochte bevat als één treffer, vóór die in de
 * tekst van de regel: zo vind je alle beurten van Noor.
 */
export function vindTreffers(
  teksten: readonly string[],
  zoek: string,
  namen: readonly (string | undefined)[] = []
): Treffer[] {
  if (zoek === '') return [];
  // De vlag u vouwt hoofdletters per teken (geen andere lengte), dus de plekken kloppen met
  // de oorspronkelijke tekst.
  const patroon = new RegExp(ontsnap(zoek), 'giu');
  const inNaam = new RegExp(ontsnap(zoek), 'iu');
  const treffers: Treffer[] = [];
  teksten.forEach((tekst, regel) => {
    const naam = namen[regel];
    if (naam !== undefined && inNaam.test(naam)) {
      treffers.push({ regel, van: 0, tot: 0, naam: true });
    }
    for (const treffer of tekst.matchAll(patroon)) {
      treffers.push({ regel, van: treffer.index, tot: treffer.index + treffer[0].length });
    }
  });
  return treffers;
}

/** Een stuk tekst van een regel, met of zonder treffer en met of zonder markering. */
export interface Deel {
  tekst: string;
  treffer: boolean;
  /** Is dit de treffer waar de gebruiker nu is (Enter gaat naar de volgende)? */
  actief: boolean;
  /** De soort persoonsgegeven als dit stuk gemarkeerd is (V-2), anders `null`. */
  soort: Soort | null;
}

/**
 * Knipt een tekst in stukken op de treffers en de markeringen van persoonsgegevens van die regel.
 * `actiefIndex` is de plek van de actieve treffer in `treffersVanRegel`, of -1. Een stuk kan
 * allebei zijn: een treffer binnen een markering blijft zichtbaar als treffer.
 */
export function deelTekst(
  tekst: string,
  treffersVanRegel: readonly Treffer[],
  actiefIndex: number,
  markeringen: readonly Markering[] = []
): Deel[] {
  const grenzen = new Set<number>([0, tekst.length]);
  for (const t of treffersVanRegel) {
    grenzen.add(t.van);
    grenzen.add(t.tot);
  }
  for (const m of markeringen) {
    grenzen.add(m.van);
    grenzen.add(m.tot);
  }
  const punten = [...grenzen].filter((g) => g >= 0 && g <= tekst.length).sort((a, b) => a - b);
  const delen: Deel[] = [];
  for (let i = 0; i + 1 < punten.length; i += 1) {
    const van = punten[i] ?? 0;
    const tot = punten[i + 1] ?? tekst.length;
    const treffer = treffersVanRegel.findIndex((t) => !t.naam && t.van <= van && t.tot >= tot);
    const markering = markeringen.find((m) => m.van <= van && m.tot >= tot);
    delen.push({
      tekst: tekst.slice(van, tot),
      treffer: treffer >= 0,
      actief: treffer >= 0 && treffer === actiefIndex,
      soort: markering?.soort ?? null
    });
  }
  return delen.length > 0 ? delen : [{ tekst, treffer: false, actief: false, soort: null }];
}

/**
 * De volgende (`1`) of vorige (`-1`) treffer vanaf `huidig`, rondlopend: na de laatste komt
 * de eerste. Zonder treffers 0.
 */
export function naarTreffer(huidig: number, aantal: number, richting: 1 | -1): number {
  if (aantal <= 0) return 0;
  return (((huidig + richting) % aantal) + aantal) % aantal;
}

// ---------------------------------------------------------------------------
// Bewerken
// ---------------------------------------------------------------------------

/** Wat een toets in het tekstveld van een regel doet. */
export type BewerkToets = 'bewaar-volgende' | 'annuleer' | 'niets';

/**
 * Enter bewaart en gaat naar de volgende regel; Shift+Enter is een nieuwe regel (het veld doet
 * dat zelf); Esc annuleert. Tijdens het typen met een toetsenbordmethode (IME) doet Enter niets.
 */
export function bewerkToets(
  toets: string,
  shift: boolean,
  tekentSamenstellen: boolean
): BewerkToets {
  if (tekentSamenstellen) return 'niets';
  if (toets === 'Escape') return 'annuleer';
  if (toets === 'Enter' && !shift) return 'bewaar-volgende';
  return 'niets';
}

/** De sleutel van de regel na `sleutel`, of `null` als dit de laatste is. */
export function volgendeSleutel(regels: readonly VerslagRegel[], sleutel: number): number | null {
  const plek = regels.findIndex((r) => r.sleutel === sleutel);
  return plek >= 0 && plek + 1 < regels.length ? (regels[plek + 1]?.sleutel ?? null) : null;
}

// ---------------------------------------------------------------------------
// Afspelen
// ---------------------------------------------------------------------------

/** De afspeelsnelheid in tienden: 10 is gewoon, 15 anderhalf keer, 20 twee keer. */
export const SNELHEDEN = [10, 15, 20] as const;
export type Snelheid = (typeof SNELHEDEN)[number];

/** Een waarde uit de instellingen als snelheid; alles buiten de drie keuzes is gewoon (10). */
export function alsSnelheid(waarde: unknown): Snelheid {
  return SNELHEDEN.find((s) => s === waarde) ?? 10;
}

/** De factor voor `playbackRate`: 1, 1,5 of 2. */
export function snelheidFactor(snelheid: Snelheid): number {
  return snelheid / 10;
}

/**
 * Zoveel milliseconden speelt de interface langer door dan het eind van de regel: het model
 * zet het eind vaak net voor de laatste klank, en een regel die midden in een woord stopt,
 * is lastig te controleren.
 */
export const UITLOOP_MS = 250;

/**
 * Is `doel` een plek waar spatie een spatie typt of een knop bedient? Dan speelt spatie niet af.
 * Een knop of link bedient spatie zelf; een tekstveld typt hem.
 */
export function neemtSpatieOver(doel: unknown): boolean {
  if (typeof Element === 'undefined' || !(doel instanceof Element)) return false;
  if (doel.closest('input, textarea, select, button, a[href], summary, [contenteditable]')) {
    return true;
  }
  return (
    doel.closest('[role="button"], [role="switch"], [role="checkbox"], [role="radio"]') !== null
  );
}

/** Wat een toets in het venster doet voor de lijst: het zoekveld openen of afspelen/pauzeren. */
export type LijstToets = 'zoeken' | 'afspelen' | null;

/**
 * Bepaalt wat een toets doet voor de verslaglijst. Een lijst die niet in beeld is (het verslag
 * blijft geladen terwijl het tabblad Notulen open staat) doet niets: anders start spatie
 * onzichtbaar geluid en opent Ctrl+F (⌘F op de Mac) een zoekveld dat niemand ziet. Spatie speelt
 * alleen als er geluid is en nooit in een tekstveld of op een knop.
 */
export function lijstToets(
  toets: {
    key: string;
    ctrlKey: boolean;
    metaKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    target: unknown;
  },
  opties: { actief: boolean; heeftGeluid: boolean }
): LijstToets {
  if (!opties.actief) return null;
  if ((toets.ctrlKey || toets.metaKey) && !toets.altKey && toets.key.toLowerCase() === 'f') {
    return 'zoeken';
  }
  if (
    toets.key === ' ' &&
    !toets.ctrlKey &&
    !toets.metaKey &&
    !toets.altKey &&
    !toets.shiftKey &&
    opties.heeftGeluid &&
    !neemtSpatieOver(toets.target)
  ) {
    return 'afspelen';
  }
  return null;
}

/** De bewaarde afspeelsnelheid. Bij een fout de gewone snelheid. */
export async function leesSnelheid(): Promise<Snelheid> {
  try {
    return alsSnelheid(await invoke<number>('afspeelsnelheid_instelling'));
  } catch {
    return 10;
  }
}

/** Bewaart de afspeelsnelheid; mislukt het, dan geldt hij nog wel tot het scherm sluit. */
export async function bewaarSnelheid(snelheid: Snelheid): Promise<void> {
  try {
    await invoke<undefined>('afspeelsnelheid_zet', { snelheid });
  } catch {
    // De snelheid is er niet minder om.
  }
}
