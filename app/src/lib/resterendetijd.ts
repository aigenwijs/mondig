/** Een meting van een download: tijdstip in milliseconden en het aantal bytes dat toen binnen was. */
export interface Meting {
  tijd: number;
  binnen: number;
}

/** Hoe ver terug we kijken voor de snelheid: een glijdend venster van tien seconden. */
const VENSTER_MS = 10_000;
/** Zo lang moet er gemeten zijn voordat we een schatting tonen; eerder springt hij te veel. */
const MIN_MEETTIJD_MS = 3_000;

/**
 * Schat uit opeenvolgende metingen hoeveel seconden de download nog duurt. De snelheid is
 * het gemiddelde over de laatste tien seconden (een glijdend gemiddelde), zodat een korte
 * dip of piek de schatting niet laat springen.
 *
 * Geeft `null` zolang er te kort gemeten is, de download stilstaat of de metingen niet
 * oplopen in tijd. Is alles binnen, dan 0.
 */
export function schatResterendeSeconden(
  metingen: readonly Meting[],
  totaal: number
): number | null {
  const laatste = metingen.at(-1);
  if (!laatste) return null;
  if (laatste.binnen >= totaal) return 0;

  const venster = metingen.filter((m) => m.tijd >= laatste.tijd - VENSTER_MS);
  const eerste = venster[0];
  if (!eerste) return null;
  const duur = laatste.tijd - eerste.tijd;
  if (duur < MIN_MEETTIJD_MS) return null;

  const bytesPerMs = (laatste.binnen - eerste.binnen) / duur;
  if (bytesPerMs <= 0) return null;
  return (totaal - laatste.binnen) / bytesPerMs / 1000;
}

/** De berichten die de tijdtekst nodig heeft; `Berichten` voldoet hieraan. */
export interface TijdBerichten {
  nogSeconden: (n: number) => string;
  nogMinuten: (n: number) => string;
  nogUren: (n: number) => string;
}

/**
 * Zet een aantal seconden om in een zin in gewone taal, afgerond op wat je kunt onthouden:
 * "Nog ongeveer 40 seconden", "Nog ongeveer 1 minuut", "Nog ongeveer 3 minuten". Onder een
 * minuut in stappen van vijf seconden, daarboven in hele minuten, vanaf anderhalf uur in uren.
 */
export function resterendeTijdTekst(seconden: number, berichten: TijdBerichten): string {
  if (seconden < 58) {
    return berichten.nogSeconden(Math.max(5, Math.round(seconden / 5) * 5));
  }
  const minuten = Math.round(seconden / 60);
  if (minuten < 90) return berichten.nogMinuten(minuten);
  return berichten.nogUren(Math.round(minuten / 60));
}
