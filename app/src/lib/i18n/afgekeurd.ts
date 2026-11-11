// De lijst afgekeurde woorden van het ontwerpsysteem (docs/ontwerp/ontwerpsysteem.md, §3), gedeeld
// door de controle op `nl.ts` (woorden.test.ts) en die op de teksten uit Rust (woorden-rust.test.ts).

export interface Regel {
  /** Naam voor in de foutmelding. */
  naam: string;
  patroon: RegExp;
  /** Wat het moet zijn (staat in de foutmelding). */
  beter: string;
}

export const AFGEKEURD: Regel[] = [
  { naam: 'wis', patroon: /\bwis\b/i, beter: 'verwijderen' },
  { naam: 'wist', patroon: /\bwist\b/i, beter: 'verwijdert' },
  { naam: 'wissen', patroon: /\bwissen\b/i, beter: 'verwijderen' },
  { naam: 'gewist', patroon: /\bgewist\b/i, beter: 'verwijderd' },
  { naam: 'weggooi', patroon: /weggooi|weggegooid/i, beter: 'verwijderen' },
  {
    naam: 'transcript',
    patroon: /transcript/i,
    beter: 'uitschrift (Bestanden) of verslag (Opnemen)'
  },
  { naam: 'deze pc', patroon: /\bdeze pc\b/i, beter: 'op deze computer' },
  { naam: 'Fysieke vergadering', patroon: /fysieke vergadering/i, beter: 'In de ruimte' },
  { naam: 'Map tonen', patroon: /\bmap tonen\b/i, beter: 'Tonen in Verkenner' },
  { naam: 'Lokaal verwerkt', patroon: /lokaal verwerkt/i, beter: 'op deze computer' },
  { naam: 'Stopwoorden', patroon: /stopwoord/i, beter: 'Opvulklanken' },
  { naam: 'opnemen tot', patroon: /\bopnemen tot\b/i, beter: 'dicteren tot' },
  { naam: 'weghalen', patroon: /weghaal|weghalen/i, beter: 'verwijderen (of uit de rij halen)' },
  { naam: 'afbreken', patroon: /afbreken|afbreek/i, beter: 'stoppen of annuleren' },
  { naam: 'exporteren', patroon: /exporteer|exporteren/i, beter: 'opslaan' },
  { naam: 'Uitschriften openen', patroon: /uitschriften openen/i, beter: 'Map openen' },
  { naam: 'VAD', patroon: /\bVAD\b/, beter: 'hulpmodel' },
  { naam: 'diarisatie', patroon: /diaris/i, beter: 'sprekermodel' }
];

export function vindAfgekeurd(tekst: string): Regel[] {
  return AFGEKEURD.filter((r) => r.patroon.test(tekst));
}
