/**
 * Maakt een aantal bytes leesbaar in gewone eenheden. Decimaal (1 MB is 1.000.000 bytes),
 * zoals `leesbare_grootte` in mondig-cli. Nederlands schrijft een komma ("1,1 GB"),
 * Engels een punt ("1.1 GB").
 */
export function leesbareGrootte(bytes: number, taal: 'nl' | 'en' = 'nl'): string {
  // Vanaf 100 GB is een decimaal ruis ("212 GB" in plaats van "212,0 GB").
  if (bytes >= 100_000_000_000) return `${String(Math.floor(bytes / 1_000_000_000))} GB`;
  if (bytes >= 1_000_000_000) {
    const tienden = Math.floor(bytes / 100_000_000);
    const scheiding = taal === 'nl' ? ',' : '.';
    return `${String(Math.floor(tienden / 10))}${scheiding}${String(tienden % 10)} GB`;
  }
  return `${String(Math.floor((bytes + 500_000) / 1_000_000))} MB`;
}

/** Zet een getal in tienden om naar tekst met de decimaalscheiding van de taal. */
function metTienden(tienden: number, taal: 'nl' | 'en'): string {
  const scheiding = taal === 'nl' ? ',' : '.';
  return `${String(Math.floor(tienden / 10))}${scheiding}${String(tienden % 10)}`;
}

/**
 * Twee groottes met één eenheid erachter, zoals "310 van 740 MB": de eenheid volgt het
 * totaal, zodat beide getallen vergelijkbaar blijven ("0,6 van 1,6 GB"). `binnen` en
 * `totaal` zijn bytes; de woorden eromheen komen uit het bericht van de aanroeper.
 */
export function leesbareVoortgang(
  binnen: number,
  totaal: number,
  taal: 'nl' | 'en' = 'nl'
): { binnen: string; totaal: string } {
  if (totaal >= 1_000_000_000) {
    return {
      binnen: metTienden(Math.floor(binnen / 100_000_000), taal),
      totaal: leesbareGrootte(totaal, taal)
    };
  }
  return {
    binnen: String(Math.floor((binnen + 500_000) / 1_000_000)),
    totaal: leesbareGrootte(totaal, taal)
  };
}
