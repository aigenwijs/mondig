// Vermeldingen van derden (cargo-about en npm), door scripts/vermeldingen.mjs gemaakt tijdens
// het bouwen. Het bestand gaat niet in git; zonder bouwstap (bijvoorbeeld `vite dev`) is het er
// niet, en dan zeggen we dat in plaats van een lege lijst te tonen.

/** Eén onderdeel van een ander. Teksten staan in `Vermeldingen.teksten`, hier alleen indexen. */
export interface Onderdeel {
  naam: string;
  versie: string;
  bron: 'rust' | 'npm' | 'overig';
  licentie: string;
  teksten: number[];
  notice: number | null;
  url: string | null;
}

export interface Vermeldingen {
  gegenereerd: string;
  teksten: string[];
  onderdelen: Onderdeel[];
}

const bestanden = import.meta.glob<Vermeldingen>('./gegenereerd/vermeldingen.json', {
  import: 'default'
});

/** Laadt de vermeldingen, of `null` als deze bouw ze niet bevat. */
export async function laadVermeldingen(): Promise<Vermeldingen | null> {
  const laad = Object.values(bestanden)[0];
  return laad ? await laad() : null;
}

/** Onderdelen waarvan de naam, versie of licentie de zoekterm bevat (hoofdletterloos). */
export function zoekOnderdelen(onderdelen: readonly Onderdeel[], zoekterm: string): Onderdeel[] {
  const term = zoekterm.trim().toLowerCase();
  if (term === '') return [...onderdelen];
  return onderdelen.filter((o) =>
    `${o.naam} ${o.versie} ${o.licentie}`.toLowerCase().includes(term)
  );
}
