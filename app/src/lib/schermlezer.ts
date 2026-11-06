/** De percentages waarbij een schermlezer een zin krijgt. Elk procent melden is ruis. */
export const DREMPELS = [25, 50, 75, 100] as const;

export type Drempel = (typeof DREMPELS)[number];

/**
 * Welke drempel is er gepasseerd tussen `vorig` en `nu` (beide in hele procenten)? Geeft de
 * hoogste die gepasseerd is, of `null` als er geen is. Springt de voortgang over meer dan één
 * drempel, dan hoort de gebruiker alleen de laatste: de eerdere zijn dan achterhaald.
 */
export function bereikteDrempel(vorig: number, nu: number): Drempel | null {
  let gevonden: Drempel | null = null;
  for (const drempel of DREMPELS) {
    if (vorig < drempel && nu >= drempel) gevonden = drempel;
  }
  return gevonden;
}
