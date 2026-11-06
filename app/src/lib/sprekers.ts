import type { ModelStand } from './modellen';

// De poort voor "Sprekers herkennen" (4b-D, fase D-2): eerst vragen, dan downloaden met
// voortgang, en pas als het sprekermodel er helemaal is gaat de schakelaar aan. Weigeren laat
// hem uit. Het patroon komt van Vibe (`providers/model-gates.ts`, MIT); hier staat alleen de
// beslislogica, zodat ze zonder scherm te testen is. De toestand zelf staat in
// `sprekers.svelte.ts`.

/** Het id van het sprekermodel in de catalogus (Rust: `model::SPREKERMODEL`). */
export const SPREKERMODEL = 'sortformer-4spk-v2.1-q8';

/** In welke fase de poort zit. */
export type PoortFase =
  /** Er gebeurt niets. */
  | 'rust'
  /** De vraag "Sprekermodel downloaden?" staat open. */
  | 'vraag'
  /** De gebruiker koos downloaden; de schakelaar gaat aan zodra het model er is. */
  | 'wacht';

/**
 * Staat de schakelaar aan? Alleen als de instelling aan staat en het model er helemaal is: de
 * instelling staat in Rust standaard aan, maar zonder model gebeurt er niets, en dan moet de
 * schakelaar dat ook zeggen.
 */
export function schakelaarAan(aan: boolean, status: ModelStand | undefined): boolean {
  return aan && status === 'compleet';
}

/** Wat aanzetten doet: meteen bewaren als het model er is, anders eerst vragen om de download. */
export function bijAanzetten(status: ModelStand | undefined): 'bewaar' | 'vraag' {
  return status === 'compleet' ? 'bewaar' : 'vraag';
}

/** Wat de poort doet als de stand van het model verandert terwijl hij wacht. */
export type NaModelwijziging = 'aanzetten' | 'opgeven' | 'blijf';

/**
 * Het model is er helemaal: aanzetten. Loopt de download niet meer (mislukt of gestopt) terwijl
 * we hem zelf begonnen, en is het model er niet: opgeven, de schakelaar blijft uit. Anders
 * (de download loopt, of is nog niet begonnen) blijven we wachten.
 */
export function naModelwijziging(
  fase: PoortFase,
  status: ModelStand | undefined,
  downloadLoopt: boolean,
  gestart: boolean
): NaModelwijziging {
  if (fase !== 'wacht') return 'blijf';
  if (status === 'compleet') return 'aanzetten';
  return gestart && !downloadLoopt ? 'opgeven' : 'blijf';
}

/**
 * Wat er onder de schakelaar staat: niets, de vraag, de stand van de download, of de bevestiging
 * dat het model er is en de schakelaar aan ging. De stand staat er vanaf de klik op Downloaden
 * (eerst "wordt gestart", dan wachten in de rij, voortgang of een nieuwe poging), zodat er nooit
 * een moment is waarop de vraag weg is en er niets voor in de plaats staat.
 */
export type PoortWeergave = 'niets' | 'vraag' | 'voortgang' | 'aangezet';

export function weergave(fase: PoortFase, netAangezet: boolean): PoortWeergave {
  if (fase === 'vraag') return 'vraag';
  if (fase === 'wacht') return 'voortgang';
  return netAangezet ? 'aangezet' : 'niets';
}
