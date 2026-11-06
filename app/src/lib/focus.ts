/**
 * Zet de focus terug als hij verloren ging. Verdwijnt de knop met focus bij een
 * toestandswissel (zoals "Annuleren" als de proef klaar is), dan valt de focus naar het
 * begin van de pagina en moet een toetsenbordgebruiker opnieuw beginnen met tabben. Dan
 * gaat de focus naar de hoofdactie van de nieuwe toestand (`data-hoofdactie`). Heeft
 * iets anders de focus, dan blijft die waar hij is.
 */
export function herstelFocus(document: Document): void {
  const actief = document.activeElement;
  if (actief && actief !== document.body) return;
  document.querySelector<HTMLElement>('[data-hoofdactie]')?.focus();
}
