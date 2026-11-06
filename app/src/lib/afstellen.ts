import type { AdviesInvoer, AdviesUitkomst, ComputerInfo } from './computer';
import { leesbareGrootte } from './grootte';
import type { Berichten, Taal } from './i18n/typen';
import type { ModelRegel } from './modellen';
import { keerGetal } from './videokaart';

// "Mondig opnieuw afstellen" (N-C5): de regels erachter. Het advies zelf rekent Rust uit
// (`hardware::adviseer`); hier wordt het een lijst met vinkjes, en na "Advies overnemen" een zin
// over wat er gebeurd is. Het scherm staat in `InstellingComputer.svelte`.

/** Waar het scherm van "Mondig opnieuw afstellen" is. */
export type AfstelToestand = 'rust' | 'bezig' | 'advies' | 'overgenomen' | 'goed';

/** De stappen van de toestand "bezig", in volgorde. */
export type AfstelStap = 'computer' | 'kaart' | 'modellen';

/** Eén regel van het advies. Een verbetering heeft een vinkje; de rest staat er om te laten zien dat Mondig keek. */
export interface AdviesRegel {
  /** Vaste naam, voor de sleutel in de lijst en voor het vinkje. */
  sleutel: string;
  titel: string;
  uitleg: string;
  /** Wat er gebeurt als de regel aangevinkt is; `null` voor een regel zonder vinkje. */
  invoer: AdviesInvoer | null;
  /** Is dit een probleem waardoor het nooit "Alles staat al goed" mag worden (videokaart niet getest)? */
  probleem?: boolean;
}

/** Wat het advies nodig heeft om de lijst te maken, naast de uitkomst van `computer_check`. */
export interface AdviesOmgeving {
  /** Staat de videokaart al aan? */
  kaartAan: boolean;
  /** De modellen met hun grootte en wat er al staat; `null` zolang het overzicht niet binnen is. */
  modellen: readonly ModelRegel[] | null;
  /** Wacht dit model in de downloadrij of komt het nu binnen? */
  loopt: (id: string) => boolean;
  taal: Taal;
  /** De reden als de snelheidstest van de videokaart niet lukte, anders `null`. */
  kaartFout?: string | null;
}

/**
 * Het deel van de uitleg dat zegt wat het kost om dit model te krijgen: de grootte van de
 * download, "staat al op je computer" of "wordt al gedownload". Een tijdsindicatie staat er
 * bewust niet bij: Mondig meet de verbinding niet, dus er is niets om hem op te baseren.
 */
export function downloadZin(id: string, omgeving: AdviesOmgeving, b: Berichten): string {
  const regel = omgeving.modellen?.find((m) => m.id === id);
  if (regel?.status === 'compleet') return b.afstelStaatEr;
  if (omgeving.loopt(id)) return b.afstelLoopt;
  if (!regel) return '';
  const nog = Math.max(0, regel.grootteBytes - regel.aanwezigBytes);
  return b.afstelDownloaden(leesbareGrootte(nog, omgeving.taal));
}

/** De reden zonder punt erachter, want de zin eromheen zet er zelf een. */
function zonderPunt(tekst: string): string {
  return tekst.replace(/[.\s]+$/, '');
}

function samen(...delen: string[]): string {
  return delen.filter((d) => d !== '').join(' ');
}

/**
 * De regels van het advies voor deze computer: eerst de verbeteringen (standaard aangevinkt),
 * dan wat al goed staat. Een lege lijst met verbeteringen is "Alles staat al goed".
 *
 * - Videokaart: aanzetten alleen als de proef winst liet zien en hij nog uit staat.
 * - Dicteren: het aangeraden model, als Mondig nu een ander gebruikt.
 * - Opnames en bestanden: alleen het nauwkeurige model als upgrade; wie bewust een ander model
 *   koos op een computer met weinig geheugen, krijgt geen "downgrade" aangeraden.
 */
export function adviesRegels(
  info: ComputerInfo,
  omgeving: AdviesOmgeving,
  b: Berichten
): AdviesRegel[] {
  const { advies } = info;
  const verbeteringen: AdviesRegel[] = [];
  const goed: AdviesRegel[] = [];
  const probleem: AdviesRegel[] = [];

  switch (advies.videokaart.stand) {
    case 'aanzetten':
      if (omgeving.kaartAan) {
        goed.push({
          sleutel: 'kaart',
          titel: b.afstelKaartGoedTitel,
          uitleg: b.afstelKaartGoedUitleg,
          invoer: null
        });
      } else {
        verbeteringen.push({
          sleutel: 'kaart',
          titel: b.afstelKaartTitel,
          uitleg: b.afstelKaartUitleg(keerGetal(advies.videokaart.keerSneller ?? 0, omgeving.taal)),
          invoer: { soort: 'videokaart' }
        });
      }
      break;
    case 'laten':
      // Staat de kaart toch aan, dan is dat de keuze van de gebruiker; Mondig raadt niet af.
      if (!omgeving.kaartAan) {
        goed.push({
          sleutel: 'kaart',
          titel: b.afstelKaartUitTitel,
          uitleg: b.afstelKaartUitUitleg,
          invoer: null
        });
      }
      break;
    case 'testen':
      // De proef is er niet gekomen (hij lukte niet): Mondig weet het niet, en zegt dat.
      probleem.push({
        sleutel: 'kaart',
        titel: b.afstelKaartNietGetestTitel,
        uitleg: b.afstelKaartNietGetestUitleg(
          zonderPunt(omgeving.kaartFout ?? b.somsGingErIetsMis)
        ),
        invoer: null,
        probleem: true
      });
      break;
    case 'faalde':
      probleem.push({
        sleutel: 'kaart',
        titel: b.afstelKaartNietGetestTitel,
        uitleg: b.afstelKaartFaaldeUitleg,
        invoer: null,
        probleem: true
      });
      break;
    default:
      break;
  }

  if (advies.dicteren !== info.dicterenInGebruik) {
    verbeteringen.push({
      sleutel: 'dicteren',
      titel: b.afstelDicterenTitel,
      uitleg: samen(b.afstelDicterenUitleg, downloadZin(advies.dicteren, omgeving, b)),
      invoer: { soort: 'model', gebruik: 'dicteren', model: advies.dicteren }
    });
  } else {
    goed.push({
      sleutel: 'dicteren',
      titel: b.afstelDicterenGoedTitel,
      uitleg: b.afstelDicterenGoedUitleg,
      invoer: null
    });
  }

  if (advies.bestanden === info.bestandenInGebruik) {
    goed.push({
      sleutel: 'bestanden',
      titel: b.afstelBestandenGoedTitel,
      uitleg: advies.bestandenApart ? b.afstelBestandenGoedUitleg : b.afstelBestandenGoedUitlegEen,
      invoer: null
    });
  } else if (advies.bestandenApart) {
    verbeteringen.push({
      sleutel: 'bestanden',
      titel: b.afstelBestandenTitel,
      uitleg: samen(b.afstelBestandenUitleg, downloadZin(advies.bestanden, omgeving, b)),
      invoer: { soort: 'model', gebruik: 'bestanden', model: advies.bestanden }
    });
  }

  return [...verbeteringen, ...probleem, ...goed];
}

/** Is er iets te verbeteren? Anders is het "Alles staat al goed". */
export function heeftVerbeteringen(regels: readonly AdviesRegel[]): boolean {
  return regels.some((r) => r.invoer !== null);
}

/** Valt er iets op te merken dat "Alles staat al goed" onwaar maakt (verbetering of probleem)? */
export function heeftIetsTeMelden(regels: readonly AdviesRegel[]): boolean {
  return regels.some((r) => r.invoer !== null || r.probleem === true);
}

/** Dezelfde marge als `download::ruimtetekort` in Rust: 20 procent, minstens 100 MB. */
function ruimteTekort(vrij: number | null, nodig: number): number {
  if (vrij === null) return 0;
  return Math.max(0, nodig + Math.max(nodig * 0.2, 100_000_000) - vrij);
}

/**
 * Passen de aangevinkte downloads samen op de schijf, samen met wat al in de rij staat? Zo niet,
 * dan de melding die dat vooraf zegt; "Advies overnemen" kan dan niet. Rust controleert het nog
 * eens bij het overnemen.
 */
export function ruimteMelding(
  regels: readonly AdviesRegel[],
  uit: ReadonlySet<string>,
  vrijBytes: number | null,
  omgeving: AdviesOmgeving,
  b: Berichten
): string | null {
  const gekozen = aangevinkt(regels, uit).flatMap((i) => (i.soort === 'model' ? [i.model] : []));
  const ids = new Set<string>();
  for (const m of omgeving.modellen ?? []) {
    if (m.status === 'compleet') continue;
    if (gekozen.includes(m.id) || omgeving.loopt(m.id)) ids.add(m.id);
  }
  if (!gekozen.some((id) => ids.has(id))) return null;
  let nodig = 0;
  for (const m of omgeving.modellen ?? []) {
    if (ids.has(m.id)) nodig += Math.max(0, m.grootteBytes - m.aanwezigBytes);
  }
  if (ruimteTekort(vrijBytes, nodig) <= 0) return null;
  return ids.size > 1 ? b.afstelGeenRuimteBeide : b.afstelGeenRuimteEen;
}

/** De punten die aangevinkt zijn, voor `advies_overnemen`. `uit` bevat de sleutels waar het vinkje weg is. */
export function aangevinkt(
  regels: readonly AdviesRegel[],
  uit: ReadonlySet<string>
): AdviesInvoer[] {
  return regels.flatMap((r) => (r.invoer !== null && !uit.has(r.sleutel) ? [r.invoer] : []));
}

/**
 * Wat er gebeurd is, in gewone taal: de videokaart staat aan, een model is meteen in gebruik
 * genomen, een model wordt gedownload (tot het klaar is blijft het huidige in gebruik). Zonder
 * modelnamen: die staan alleen op Modellen (N-B), hier de rol.
 */
export function overgenomenTekst(uitkomst: AdviesUitkomst, b: Berichten): string {
  const zinnen: string[] = [];
  if (uitkomst.videokaartAan) zinnen.push(b.computerCheckKaartAan);
  for (const w of uitkomst.gewisseld) {
    zinnen.push(
      w.gebruik === 'dicteren' ? b.afstelOvergenomenDicteren : b.afstelOvergenomenBestanden
    );
  }
  for (const d of uitkomst.downloadt) {
    const welk = d.taken.includes('dicteren')
      ? d.taken.includes('bestanden')
        ? b.afstelRolModel
        : b.afstelRolSnel
      : b.afstelRolNauwkeurig;
    zinnen.push(b.afstelOvergenomenDownload(welk));
  }
  return zinnen.length > 0 ? zinnen.join(' ') : b.afstelOvergenomenNiets;
}

/** "4 november": de datum van de controle, in de taal van de app. */
export function controleDatum(nu: Date, taal: Taal): string {
  return nu.toLocaleDateString(taal === 'nl' ? 'nl-NL' : 'en-GB', {
    day: 'numeric',
    month: 'long'
  });
}
