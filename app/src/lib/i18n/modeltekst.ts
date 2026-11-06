/**
 * Wat de pagina Modellen over één model vertelt, in gewone taal. De feiten (grootte, labels,
 * wat niet te kiezen is) komen uit de catalogus in Rust; deze teksten horen bij de taal.
 */
export interface ModelTekst {
  /** De naam zoals de pagina die toont. */
  naam: string;
  /** Eén zin over het model. */
  zin: string;
  /** Hoe snel je tekst er staat bij dicteren, na "Dicteren:". Alleen bij een spraakmodel. */
  dicterenTijd?: string;
  /** Hoe lang een uur opname duurt. Alleen bij een spraakmodel. */
  uurOpname?: string;
  /** Een extra regel onder het model, als er iets bij te vertellen is. */
  extra?: string;
}
