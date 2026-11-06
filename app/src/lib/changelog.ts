// De changelog (CHANGELOG.md, Keep a Changelog) als gegevens voor "Wat is er nieuw".
// Alleen pure functies; de tekst zelf wordt in de interface geïmporteerd (zie wat-is-er-nieuw.ts),
// zodat dit zonder netwerk werkt.

/** De kop van de sectie met wijzigingen die nog in geen uitgave zitten. */
export const NOG_NIET_UITGEBRACHT = 'Nog niet uitgebracht';

/** Eén categorie (Toegevoegd, Gewijzigd, ...) met zijn punten. */
export interface Categorie {
  naam: string;
  punten: string[];
}

/** Eén uitgave, of de sectie "Nog niet uitgebracht". */
export interface Uitgave {
  /** Versie zonder haken, bijvoorbeeld `0.3.0`, of `Nog niet uitgebracht`. */
  kop: string;
  /** Datum zoals in de kop, als die er staat (`2026-11-05`). */
  datum: string | null;
  categorieen: Categorie[];
}

/** Haalt de opmaak uit een punt: alleen de tekst blijft over. */
function schoon(tekst: string): string {
  return tekst
    .replace(/`([^`]*)`/g, '$1')
    .replace(/\*\*([^*]*)\*\*/g, '$1')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Leest de changelog in. Nieuwste uitgave eerst, zoals in het bestand. */
export function leesChangelog(markdown: string): Uitgave[] {
  const uitgaven: Uitgave[] = [];
  let uitgave: Uitgave | null = null;
  let categorie: Categorie | null = null;
  let punt: string[] | null = null;

  const sluitPunt = (): void => {
    if (categorie && punt) categorie.punten.push(schoon(punt.join(' ')));
    punt = null;
  };

  for (const regel of markdown.replace(/\r\n/g, '\n').split('\n')) {
    const kop2 = /^##\s+\[([^\]]+)\](?:\s*-\s*(\d{4}-\d{2}-\d{2}))?\s*$/.exec(regel);
    if (kop2?.[1]) {
      sluitPunt();
      uitgave = { kop: kop2[1].trim(), datum: kop2[2] ?? null, categorieen: [] };
      uitgaven.push(uitgave);
      categorie = null;
      continue;
    }
    const kop3 = /^###\s+(.+?)\s*$/.exec(regel);
    if (kop3?.[1] && uitgave) {
      sluitPunt();
      categorie = { naam: kop3[1], punten: [] };
      uitgave.categorieen.push(categorie);
      continue;
    }
    if (!categorie) continue;
    if (regel.startsWith('- ')) {
      sluitPunt();
      punt = [regel.slice(2)];
    } else if (punt && /^\s+\S/.test(regel)) {
      punt.push(regel.trim());
    } else if (regel.trim() === '') {
      sluitPunt();
    }
  }
  sluitPunt();
  return uitgaven.map((u) => ({
    ...u,
    categorieen: u.categorieen.filter((c) => c.punten.length > 0)
  }));
}

/** Is dit een versie tussen twee uitgaven in (`0.3.0-dev`)? */
export function isDevVersie(versie: string): boolean {
  return versie.split('+')[0]?.endsWith('-dev') ?? false;
}

/**
 * De uitgave die bij deze versie hoort. Een `-dev`-versie krijgt "Nog niet uitgebracht";
 * een uitgebrachte versie de sectie met die versie, of `null` als die er niet is.
 */
export function uitgaveVoor(uitgaven: readonly Uitgave[], versie: string): Uitgave | null {
  const zonderBouw = versie.split('+')[0] ?? versie;
  const gezocht = isDevVersie(zonderBouw) ? NOG_NIET_UITGEBRACHT : zonderBouw;
  return uitgaven.find((u) => u.kop === gezocht) ?? null;
}

/** Alle punten van een uitgave achter elkaar, zonder categorieën. */
export function allePunten(uitgave: Uitgave): string[] {
  return uitgave.categorieen.flatMap((c) => c.punten);
}

/**
 * De eerste zin van een punt, voor de kleine kaart "Wat is er nieuw". De zin eindigt bij een
 * leesteken met een spatie erachter, dus een versienummer als 1.0 knipt niet.
 */
export function eersteZin(punt: string): string {
  const m = /^.*?[.!?](?=\s|$)/.exec(punt);
  return m ? m[0] : punt;
}

/** Hoeveel punten een uitgave in eerste instantie toont voordat de rest uitklapt. */
export const EERSTE_PUNTEN = 5;

/**
 * De categorieën met samen hooguit `max` punten, in volgorde; `verborgen` is het aantal dat
 * er nog bij hoort. Categorieën zonder zichtbare punten vallen weg.
 */
export function beperkPunten(
  uitgave: Uitgave,
  max: number
): { categorieen: Categorie[]; verborgen: number } {
  let over = max;
  const categorieen: Categorie[] = [];
  for (const c of uitgave.categorieen) {
    if (over <= 0) break;
    const punten = c.punten.slice(0, over);
    over -= punten.length;
    categorieen.push({ naam: c.naam, punten });
  }
  return { categorieen, verborgen: Math.max(0, allePunten(uitgave).length - max) };
}

/**
 * De uitgaven voor "Eerdere uitgaven": alles behalve de huidige, zonder lege secties. Werk dat
 * nog niet is uitgebracht telt alleen mee bij een `-dev`-versie, want dat is de huidige zelf.
 */
export function eerdereUitgaven(uitgaven: readonly Uitgave[], versie: string): Uitgave[] {
  const huidige = uitgaveVoor(uitgaven, versie);
  return uitgaven.filter(
    (u) => u !== huidige && u.kop !== NOG_NIET_UITGEBRACHT && allePunten(u).length > 0
  );
}
