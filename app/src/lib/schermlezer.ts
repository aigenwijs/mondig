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

/** Hoe lang de regio leeg blijft voor dezelfde zin opnieuw komt: genoeg voor de schermlezer. */
export const MELD_PAUZE_MS = 100;

/**
 * Maakt een functie die een zin aan een `role="status"`-regio geeft, ook als het dezelfde zin is
 * als de vorige ("20 dictaten erbij" na een tweede "Meer tonen"). Een regio die dezelfde tekst
 * krijgt, verandert niet, en dan zegt de schermlezer niets: daarom eerst leegmaken, een korte
 * pauze (een schermlezer ziet een leeg-en-weer-vol binnen een beeldje vaak niet), en dan pas de
 * zin zetten. `zet` krijgt dus eerst '' en dan de zin. Een nieuwere zin vervangt een zin die nog
 * wacht.
 */
export function maakMelder(
  zet: (tekst: string) => void,
  wacht: () => Promise<void> = () =>
    new Promise((klaar) => {
      setTimeout(klaar, MELD_PAUZE_MS);
    })
): (tekst: string) => void {
  let laatste = 0;
  return (tekst) => {
    const nummer = ++laatste;
    zet('');
    void wacht().then(() => {
      if (nummer === laatste) zet(tekst);
    });
  };
}
