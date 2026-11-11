/**
 * Zet de focus terug als hij verloren ging. Verdwijnt de knop met focus bij een
 * toestandswissel (zoals "Annuleren" als de proef klaar is), dan valt de focus naar het
 * begin van de pagina en moet een toetsenbordgebruiker opnieuw beginnen met tabben. Dan
 * gaat de focus naar de hoofdactie van de nieuwe toestand (`data-hoofdactie`). Heeft
 * iets anders de focus, dan blijft die waar hij is. Standaard is het doel de hoofdactie; de
 * aanroeper kan een andere plek opgeven (bij de start van een dictaat het paneel zelf, want
 * daar is Annuleren juist niet de hoofdactie).
 */
export function herstelFocus(document: Document, doel = '[data-hoofdactie]'): void {
  const actief = document.activeElement;
  // Een vaste plek (`data-focusplek`) telt als "nergens": de focus mag van daar naar het doel.
  const opVastePlek = actief?.hasAttribute('data-focusplek') === true;
  if (actief && actief !== document.body && !opVastePlek) return;
  document.querySelector<HTMLElement>(doel)?.focus();
}
